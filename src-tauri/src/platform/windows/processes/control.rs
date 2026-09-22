//! The Windows process inspector and process control backends.
//!
//! ```text
//! pin        OpenProcess(minimum rights) + GetProcessTimes on that handle
//! terminate  TerminateProcess                         PROCESS_TERMINATE
//! suspend    Toolhelp thread snapshot → OpenThread → SuspendThread
//! resume     ResumeThread, only on the threads PULSE suspended
//! priority   GetPriorityClass / SetPriorityClass      PROCESS_SET_INFORMATION
//! affinity   GetProcessAffinityMask / SetProcessAffinityMask
//! inspect    OpenProcessToken → TokenUser → LookupAccountSidW / ConvertSidToStringSidW
//!            IsWow64Process2 (resolved at runtime) → architecture
//!            QueryFullProcessImageNameW → path → version resource
//! provenance WinVerifyTrust, embedded then catalog, offline
//! ```
//!
//! # The handle is the pin
//!
//! Windows does not recycle a PID while any handle to the process object is
//! open. PULSE opens the handle **first**, reads the creation time through
//! it, compares it with the selected instance and only then acts through the
//! same handle. There is no window in which the PID can change hands.
//!
//! # Minimum rights, never `PROCESS_ALL_ACCESS`
//!
//! | Action | Rights requested |
//! |---|---|
//! | inspect, read priority/affinity | `PROCESS_QUERY_LIMITED_INFORMATION` |
//! | terminate | `PROCESS_TERMINATE` + query-limited |
//! | priority, affinity | `PROCESS_SET_INFORMATION` + query-limited |
//! | suspend/resume | query-limited on the process, `THREAD_SUSPEND_RESUME` + `THREAD_QUERY_LIMITED_INFORMATION` per thread |
//!
//! A refusal is `permissionDenied`. PULSE never enables a privilege, never
//! asks for UAC and never retries with more rights.
//!
//! # Suspension is per thread, and PULSE only undoes its own
//!
//! There is no documented "suspend process" call; `NtSuspendProcess` is an
//! undocumented native API and deliberately not used. PULSE enumerates the
//! process's threads with `CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD)` and
//! `Thread32First`/`Thread32Next` (the thread walk has no `W` variant),
//! suspends each, and records exactly which thread IDs it added a suspension
//! to. *Resume* calls `ResumeThread` once on each of those and on nothing
//! else, so a thread a debugger or the application itself had suspended
//! stays suspended. Threads created after the suspension are not suspended;
//! that is inherent to the documented API and stated in the docs.
//!
//! # Not physically executed
//!
//! This file compiles for `x86_64-pc-windows-msvc` and its pure logic is
//! tested on Fedora (`facts`, `authenticode`, `version`), but it has **not**
//! been run on a Windows machine.

#[cfg(target_os = "windows")]
use std::sync::Arc;

#[cfg(target_os = "windows")]
use crate::processes::control::ProcessControlBackend;
#[cfg(target_os = "windows")]
use crate::processes::inspector::ProcessInspectorBackend;

/// Controls processes through minimally-privileged handles.
#[derive(Debug, Default)]
pub struct WindowsProcessControl;

/// Inspects processes through handles, tokens and the file system.
#[derive(Debug, Default)]
pub struct WindowsProcessInspector;

#[cfg(target_os = "windows")]
pub fn control_backend() -> Arc<dyn ProcessControlBackend> {
    Arc::new(WindowsProcessControl)
}

#[cfg(target_os = "windows")]
pub fn inspector_backend() -> Arc<dyn ProcessInspectorBackend> {
    Arc::new(WindowsProcessInspector)
}

#[cfg(target_os = "windows")]
mod imp {
    use std::time::UNIX_EPOCH;

    use windows_sys::Win32::Foundation::{
        CloseHandle, GetLastError, LocalFree, ERROR_ACCESS_DENIED, ERROR_INVALID_PARAMETER,
        FILETIME, HANDLE, INVALID_HANDLE_VALUE, STILL_ACTIVE,
    };
    use windows_sys::Win32::Security::Authorization::ConvertSidToStringSidW;
    use windows_sys::Win32::Security::{
        GetTokenInformation, LookupAccountSidW, TokenUser, SID_NAME_USE, TOKEN_QUERY, TOKEN_USER,
    };
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, Thread32First, Thread32Next,
        PROCESSENTRY32W, TH32CS_SNAPPROCESS, TH32CS_SNAPTHREAD, THREADENTRY32,
    };
    use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
    use windows_sys::Win32::System::SystemInformation::{GetNativeSystemInfo, SYSTEM_INFO};
    use windows_sys::Win32::System::Threading::{
        GetActiveProcessorGroupCount, GetExitCodeProcess, GetPriorityClass, GetProcessAffinityMask,
        GetProcessIdOfThread, GetProcessTimes, IsWow64Process, OpenProcess, OpenProcessToken,
        OpenThread, QueryFullProcessImageNameW, ResumeThread, SetPriorityClass,
        SetProcessAffinityMask, SuspendThread, TerminateProcess, PROCESS_QUERY_LIMITED_INFORMATION,
        PROCESS_SET_INFORMATION, PROCESS_TERMINATE, THREAD_QUERY_LIMITED_INFORMATION,
        THREAD_SUSPEND_RESUME,
    };

    use crate::metrics::model::Availability;
    use crate::processes::control::{
        Access, ControlError, PinnedProcess, PriorityKind, ProcessAffinity, ProcessControlBackend,
        ProcessNode, ProcessPriority, ResumeReport, SuspendRecord, TerminateMode, ThreadApply,
    };
    use crate::processes::field::Field;
    use crate::processes::inspector::{
        Capability, ExecutableInfo, OpenedExecutable, PlatformAccess, ProcessInspectorBackend,
        ProcessOwner, Provenance, RawDetails,
    };
    use crate::processes::{ProcessClass, ProcessState};

    use super::super::facts::{
        account_name, architecture, category_from_sid, cpus_to_mask, filetime_to_unix_ms,
        mask_to_cpus, priority_class_from_raw, priority_class_to_raw, processor_architecture_name,
        MULTI_GROUP_LIMITATION,
    };
    use super::super::{authenticode, classify, times, version, Depth, STATE_UNSUPPORTED};
    use super::{WindowsProcessControl, WindowsProcessInspector};

    /// An owned `HANDLE`, closed exactly once on drop.
    struct OwnedHandle(HANDLE);

    impl OwnedHandle {
        fn new(handle: HANDLE) -> Option<Self> {
            (!handle.is_null() && handle != INVALID_HANDLE_VALUE).then_some(Self(handle))
        }
    }

    impl Drop for OwnedHandle {
        fn drop(&mut self) {
            // SAFETY: validated at construction; this is the only owner.
            unsafe { CloseHandle(self.0) };
        }
    }

    // SAFETY: a process, thread or token handle is a kernel object reference
    // that Windows allows any thread of the owning process to use and close.
    // The wrapper has a single owner and is never shared, only moved.
    unsafe impl Send for OwnedHandle {}

    fn last_error() -> u32 {
        // SAFETY: reads the calling thread's last-error value.
        unsafe { GetLastError() }
    }

    fn wide_to_string(buffer: &[u16]) -> String {
        let end = buffer
            .iter()
            .position(|unit| *unit == 0)
            .unwrap_or(buffer.len());
        String::from_utf16_lossy(&buffer[..end])
    }

    fn error_for(code: u32, what: &str) -> ControlError {
        match code {
            ERROR_ACCESS_DENIED => ControlError::PermissionDenied(format!(
                "Windows refused {what}. PULSE runs without elevation and never requests it."
            )),
            ERROR_INVALID_PARAMETER => ControlError::Gone,
            other => ControlError::Platform(format!("{what} failed with Windows error {other}.")),
        }
    }

    /// Opens `pid` with exactly `rights`.
    fn open(pid: u32, rights: u32, what: &str) -> Result<OwnedHandle, ControlError> {
        // SAFETY: integer arguments; the result is validated.
        let handle = unsafe { OpenProcess(rights, 0, pid) };
        OwnedHandle::new(handle).ok_or_else(|| error_for(last_error(), what))
    }

    fn has_exited(handle: &OwnedHandle) -> bool {
        let mut code: u32 = 0;
        // SAFETY: a live handle with query rights and a live out-parameter.
        let ok = unsafe { GetExitCodeProcess(handle.0, &mut code) };
        ok != 0 && code != STILL_ACTIVE as u32
    }

    /// The creation FILETIME, read through the handle — the start token.
    fn creation_time(handle: &OwnedHandle) -> Result<u64, ControlError> {
        let mut creation: FILETIME = unsafe { std::mem::zeroed() };
        let mut exit: FILETIME = unsafe { std::mem::zeroed() };
        let mut kernel: FILETIME = unsafe { std::mem::zeroed() };
        let mut user: FILETIME = unsafe { std::mem::zeroed() };
        // SAFETY: four live FILETIME locals and a handle with query rights.
        let ok =
            unsafe { GetProcessTimes(handle.0, &mut creation, &mut exit, &mut kernel, &mut user) };
        if ok == 0 {
            return Err(error_for(last_error(), "reading the process creation time"));
        }
        Ok(times::start_token(
            creation.dwLowDateTime,
            creation.dwHighDateTime,
        ))
    }

    fn image_path(handle: &OwnedHandle) -> Result<String, ControlError> {
        let mut buffer = vec![0_u16; 32_768];
        let mut size = buffer.len() as u32;
        // SAFETY: the buffer is live and `size` carries its capacity.
        let ok = unsafe { QueryFullProcessImageNameW(handle.0, 0, buffer.as_mut_ptr(), &mut size) };
        if ok == 0 || size == 0 || size as usize > buffer.len() {
            return Err(error_for(last_error(), "naming the executable"));
        }
        Ok(String::from_utf16_lossy(&buffer[..size as usize]))
    }

    fn rights(access: Access) -> u32 {
        PROCESS_QUERY_LIMITED_INFORMATION
            | match access {
                Access::Query | Access::SuspendResume => 0,
                Access::Terminate => PROCESS_TERMINATE,
                Access::SetPriority | Access::SetAffinity => PROCESS_SET_INFORMATION,
            }
    }

    /// Opens and validates a process: exists, has not exited, and reports its
    /// start token through the very handle that pins it.
    fn pin_handle(pid: u32, access: Access) -> Result<(OwnedHandle, u64), ControlError> {
        let handle = open(pid, rights(access), "opening the process")?;
        if has_exited(&handle) {
            return Err(ControlError::Gone);
        }
        let token = creation_time(&handle)?;
        Ok((handle, token))
    }

    // --- threads -------------------------------------------------------------

    /// The IDs of `pid`'s threads, from one Toolhelp snapshot.
    fn thread_ids(pid: u32) -> Result<Vec<u32>, ControlError> {
        // SAFETY: integer arguments; the result is validated.
        let snapshot = OwnedHandle::new(unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) })
            .ok_or_else(|| error_for(last_error(), "listing threads"))?;

        let mut entry: THREADENTRY32 = unsafe { std::mem::zeroed() };
        entry.dwSize = std::mem::size_of::<THREADENTRY32>() as u32;

        let mut ids = Vec::new();
        // SAFETY: a live snapshot and a correctly sized entry.
        if unsafe { Thread32First(snapshot.0, &mut entry) } == 0 {
            return Ok(ids);
        }
        loop {
            if entry.th32OwnerProcessID == pid {
                ids.push(entry.th32ThreadID);
            }
            // SAFETY: as above.
            if unsafe { Thread32Next(snapshot.0, &mut entry) } == 0 {
                break;
            }
        }
        Ok(ids)
    }

    enum ThreadOpen {
        Opened(OwnedHandle),
        Gone,
        Refused,
    }

    /// Opens one thread and proves it still belongs to `pid`.
    ///
    /// The caller holds a handle to `pid`, so that PID cannot have been
    /// recycled; a thread whose owner is `pid` is therefore a thread of the
    /// pinned process, even if its TID was recycled from an earlier thread.
    fn open_thread(tid: u32, pid: u32) -> ThreadOpen {
        // SAFETY: integer arguments; the result is validated.
        let handle = unsafe {
            OpenThread(
                THREAD_SUSPEND_RESUME | THREAD_QUERY_LIMITED_INFORMATION,
                0,
                tid,
            )
        };
        let Some(handle) = OwnedHandle::new(handle) else {
            return match last_error() {
                ERROR_INVALID_PARAMETER => ThreadOpen::Gone,
                _ => ThreadOpen::Refused,
            };
        };
        // SAFETY: a live thread handle with THREAD_QUERY_LIMITED_INFORMATION.
        if unsafe { GetProcessIdOfThread(handle.0) } != pid {
            return ThreadOpen::Gone;
        }
        ThreadOpen::Opened(handle)
    }

    // --- the pinned process --------------------------------------------------

    struct WindowsPinned {
        pid: u32,
        handle: OwnedHandle,
        token: u64,
    }

    impl PinnedProcess for WindowsPinned {
        fn start_token(&self) -> u64 {
            self.token
        }

        fn is_stopped(&self) -> bool {
            false
        }

        fn terminate(&self, mode: TerminateMode) -> Result<(), ControlError> {
            if mode == TerminateMode::Force {
                return Err(ControlError::Unsupported(
                    "Windows termination is already immediate.".to_string(),
                ));
            }
            // SAFETY: a live handle carrying PROCESS_TERMINATE.
            if unsafe { TerminateProcess(self.handle.0, 1) } != 0 {
                return Ok(());
            }
            let code = last_error();
            // A process already exiting refuses termination with
            // ERROR_ACCESS_DENIED; that is "gone", not "refused".
            if has_exited(&self.handle) {
                return Err(ControlError::Gone);
            }
            Err(error_for(code, "ending the process"))
        }

        fn suspend(&self) -> Result<SuspendRecord, ControlError> {
            let mut record = SuspendRecord::default();
            for tid in thread_ids(self.pid)? {
                match open_thread(tid, self.pid) {
                    ThreadOpen::Opened(thread) => {
                        // SAFETY: a live handle with THREAD_SUSPEND_RESUME.
                        if unsafe { SuspendThread(thread.0) } == u32::MAX {
                            record.failed_threads += 1;
                        } else {
                            record.thread_ids.push(tid);
                        }
                    }
                    ThreadOpen::Gone => {}
                    ThreadOpen::Refused => record.failed_threads += 1,
                }
            }

            if record.thread_ids.is_empty() {
                return if record.failed_threads > 0 {
                    Err(ControlError::PermissionDenied(
                        "Windows refused to suspend every thread of this process.".to_string(),
                    ))
                } else {
                    Err(ControlError::Gone)
                };
            }
            Ok(record)
        }

        fn resume(&self, record: &SuspendRecord) -> Result<ResumeReport, ControlError> {
            let mut report = ResumeReport::default();
            for tid in &record.thread_ids {
                match open_thread(*tid, self.pid) {
                    ThreadOpen::Opened(thread) => {
                        // SAFETY: a live handle with THREAD_SUSPEND_RESUME.
                        // Exactly one call: PULSE added exactly one suspension.
                        if unsafe { ResumeThread(thread.0) } == u32::MAX {
                            report.failed += 1;
                        } else {
                            report.resumed += 1;
                        }
                    }
                    ThreadOpen::Gone => report.gone += 1,
                    ThreadOpen::Refused => report.failed += 1,
                }
            }
            Ok(report)
        }

        fn priority(&self) -> Result<ProcessPriority, ControlError> {
            read_priority(&self.handle)
        }

        fn set_priority(&self, priority: ProcessPriority) -> Result<ThreadApply, ControlError> {
            let ProcessPriority::WindowsClass { class } = priority else {
                return Err(ControlError::Invalid(
                    "Windows uses priority classes.".to_string(),
                ));
            };
            // SAFETY: a live handle with PROCESS_SET_INFORMATION.
            if unsafe { SetPriorityClass(self.handle.0, priority_class_to_raw(class)) } == 0 {
                return Err(error_for(last_error(), "changing the priority class"));
            }
            // Without SeIncreaseBasePriorityPrivilege, Windows silently
            // applies High instead of Realtime. Say so rather than claim it.
            match read_priority(&self.handle)? {
                ProcessPriority::WindowsClass { class: applied } if applied == class => {
                    Ok(ThreadApply {
                        applied: 1,
                        failed: 0,
                    })
                }
                other => Err(ControlError::PermissionDenied(format!(
                    "Windows applied {other:?} instead of the requested class; the requested \
                     class needs a privilege PULSE does not have."
                ))),
            }
        }

        fn affinity(&self) -> Result<ProcessAffinity, ControlError> {
            read_affinity(&self.handle)
        }

        fn set_affinity(&self, cpus: &[u32]) -> Result<ThreadApply, ControlError> {
            let current = read_affinity(&self.handle)?;
            if current.limitation.is_some() {
                return Err(ControlError::Unsupported(
                    MULTI_GROUP_LIMITATION.to_string(),
                ));
            }
            let system = current
                .available
                .iter()
                .fold(0_usize, |mask, cpu| mask | (1_usize << cpu));
            let mask = cpus_to_mask(cpus, system).map_err(ControlError::Invalid)?;
            // SAFETY: a live handle with PROCESS_SET_INFORMATION; the mask is
            // a non-empty subset of the system mask.
            if unsafe { SetProcessAffinityMask(self.handle.0, mask) } == 0 {
                return Err(error_for(last_error(), "changing the CPU affinity"));
            }
            Ok(ThreadApply {
                applied: 1,
                failed: 0,
            })
        }
    }

    fn read_priority(handle: &OwnedHandle) -> Result<ProcessPriority, ControlError> {
        // SAFETY: a live handle with query rights.
        let raw = unsafe { GetPriorityClass(handle.0) };
        if raw == 0 {
            return Err(error_for(last_error(), "reading the priority class"));
        }
        priority_class_from_raw(raw)
            .map(|class| ProcessPriority::WindowsClass { class })
            .ok_or_else(|| ControlError::Platform(format!("Unknown priority class 0x{raw:x}.")))
    }

    fn read_affinity(handle: &OwnedHandle) -> Result<ProcessAffinity, ControlError> {
        let mut process: usize = 0;
        let mut system: usize = 0;
        // SAFETY: a live handle with query rights and two live out-parameters.
        if unsafe { GetProcessAffinityMask(handle.0, &mut process, &mut system) } == 0 {
            return Err(error_for(last_error(), "reading the CPU affinity"));
        }
        // SAFETY: a pure query.
        let groups = unsafe { GetActiveProcessorGroupCount() };
        Ok(ProcessAffinity {
            cpus: mask_to_cpus(process),
            available: mask_to_cpus(system),
            limitation: (groups > 1).then(|| MULTI_GROUP_LIMITATION.to_string()),
        })
    }

    impl ProcessControlBackend for WindowsProcessControl {
        fn pin(&self, pid: u32, access: Access) -> Result<Box<dyn PinnedProcess>, ControlError> {
            let (handle, token) = pin_handle(pid, access)?;
            Ok(Box::new(WindowsPinned { pid, handle, token }))
        }

        fn process_nodes(&self) -> Vec<ProcessNode> {
            // A full walk: the tree planner needs every start token to tell a
            // real child from one whose parent PID was recycled.
            super::super::scan(Depth::Full)
                .processes
                .into_iter()
                .map(|process| ProcessNode {
                    instance: process.instance,
                    parent_pid: process.parent_pid,
                })
                .collect()
        }

        fn priority_kind(&self) -> PriorityKind {
            PriorityKind::WindowsClass
        }

        fn supports_force_kill(&self) -> bool {
            false
        }
    }

    // --- the inspector -------------------------------------------------------

    /// The Toolhelp entry for one PID: parent and image name.
    fn inventory(pid: u32) -> Option<(u32, String)> {
        // SAFETY: integer arguments; validated.
        let snapshot =
            OwnedHandle::new(unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) })?;
        let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        // SAFETY: a live snapshot and a correctly sized entry.
        if unsafe { Process32FirstW(snapshot.0, &mut entry) } == 0 {
            return None;
        }
        loop {
            if entry.th32ProcessID == pid {
                return Some((entry.th32ParentProcessID, wide_to_string(&entry.szExeFile)));
            }
            // SAFETY: as above.
            if unsafe { Process32NextW(snapshot.0, &mut entry) } == 0 {
                return None;
            }
        }
    }

    /// The parent's name — only when the process at the parent PID provably
    /// started before the child. Windows never updates a parent PID, so the
    /// original parent may be long gone and its PID reused.
    fn parent_name(parent: u32, child_token: u64) -> Field<String> {
        let Some((_, name)) = inventory(parent) else {
            return Field::missing(Availability::not_detected("the parent has exited"));
        };
        match pin_handle(parent, Access::Query) {
            Ok((_, token)) if token <= child_token => Field::available(name),
            Ok(_) => Field::missing(Availability::not_detected(
                "the original parent exited and its PID now belongs to a newer process",
            )),
            Err(_) => Field::missing(Availability::permission_denied(format!(
                "PID {parent} is currently '{name}', but PULSE could not verify it is the \
                 original parent"
            ))),
        }
    }

    type IsWow64Process2Fn = unsafe extern "system" fn(HANDLE, *mut u16, *mut u16) -> i32;

    /// `IsWow64Process2`, resolved at runtime so PULSE still loads on a
    /// Windows build that predates it.
    fn is_wow64_process2() -> Option<IsWow64Process2Fn> {
        let module: Vec<u16> = "kernel32.dll\0".encode_utf16().collect();
        // SAFETY: kernel32 is always loaded; a NUL-terminated name.
        let kernel32 = unsafe { GetModuleHandleW(module.as_ptr()) };
        if kernel32.is_null() {
            return None;
        }
        // SAFETY: a NUL-terminated ASCII export name.
        let address = unsafe { GetProcAddress(kernel32, c"IsWow64Process2".as_ptr().cast()) }?;
        // SAFETY: the export's documented signature matches the alias.
        Some(unsafe {
            std::mem::transmute::<unsafe extern "system" fn() -> isize, IsWow64Process2Fn>(address)
        })
    }

    fn read_architecture(handle: &OwnedHandle) -> Field<String> {
        if let Some(query) = is_wow64_process2() {
            let mut process: u16 = 0;
            let mut native: u16 = 0;
            // SAFETY: a live handle and two live out-parameters.
            if unsafe { query(handle.0, &mut process, &mut native) } != 0 {
                return match architecture(process, native) {
                    Some(name) => Field::available(name.to_string()),
                    None => Field::missing(Availability::not_detected(format!(
                        "unknown machine type 0x{process:04x}/0x{native:04x}"
                    ))),
                };
            }
        }

        // Fallback: WOW64 or not, plus the native processor.
        let mut wow64 = 0;
        // SAFETY: a live handle and out-parameter.
        if unsafe { IsWow64Process(handle.0, &mut wow64) } == 0 {
            return Field::missing(Availability::temporarily_unavailable(
                "the process architecture could not be read",
            ));
        }
        if wow64 != 0 {
            return Field::available("x86".to_string());
        }
        let mut info: SYSTEM_INFO = unsafe { std::mem::zeroed() };
        // SAFETY: a live out-structure.
        unsafe { GetNativeSystemInfo(&mut info) };
        // SAFETY: the union's first member is always initialised by the call.
        let native = unsafe { info.Anonymous.Anonymous.wProcessorArchitecture };
        match processor_architecture_name(native) {
            Some(name) => Field::available(name.to_string()),
            None => Field::missing(Availability::not_detected("unknown processor architecture")),
        }
    }

    /// The token's user: SID string and resolved account name.
    ///
    /// `LookupAccountSidW` resolves local and well-known accounts locally.
    /// On a domain-joined machine Windows itself may consult the domain
    /// controller for a domain account; PULSE issues no request of its own.
    fn read_owner(handle: &OwnedHandle) -> Field<ProcessOwner> {
        let mut token: HANDLE = std::ptr::null_mut();
        // SAFETY: a live process handle and out-parameter.
        if unsafe { OpenProcessToken(handle.0, TOKEN_QUERY, &mut token) } == 0 {
            return Field::missing(Availability::permission_denied(
                "Windows refused to open this process's token",
            ));
        }
        let Some(token) = OwnedHandle::new(token) else {
            return Field::missing(Availability::permission_denied(
                "Windows refused to open this process's token",
            ));
        };

        let mut length: u32 = 0;
        // SAFETY: a size query.
        unsafe { GetTokenInformation(token.0, TokenUser, std::ptr::null_mut(), 0, &mut length) };
        if length == 0 {
            return Field::missing(Availability::temporarily_unavailable(
                "the token user could not be read",
            ));
        }
        // u64 storage keeps the buffer aligned for TOKEN_USER.
        let mut buffer = vec![0_u64; (length as usize).div_ceil(8)];
        // SAFETY: the buffer is at least `length` bytes.
        if unsafe {
            GetTokenInformation(
                token.0,
                TokenUser,
                buffer.as_mut_ptr().cast(),
                length,
                &mut length,
            )
        } == 0
        {
            return Field::missing(Availability::temporarily_unavailable(
                "the token user could not be read",
            ));
        }
        // SAFETY: the call succeeded, so the buffer starts with a TOKEN_USER
        // whose SID points inside the same buffer.
        let sid = unsafe { (*(buffer.as_ptr() as *const TOKEN_USER)).User.Sid };

        let mut text: *mut u16 = std::ptr::null_mut();
        // SAFETY: a valid SID and an out-pointer the API allocates with
        // LocalAlloc, freed below.
        let id = if unsafe { ConvertSidToStringSidW(sid, &mut text) } != 0 && !text.is_null() {
            // SAFETY: NUL-terminated wide string allocated by the call.
            let length = (0..).take_while(|i| unsafe { *text.add(*i) } != 0).count();
            let value =
                String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(text, length) });
            // SAFETY: allocated by ConvertSidToStringSidW with LocalAlloc.
            unsafe { LocalFree(text.cast()) };
            value
        } else {
            return Field::missing(Availability::temporarily_unavailable(
                "the owner SID could not be rendered",
            ));
        };

        let mut name = [0_u16; 256];
        let mut domain = [0_u16; 256];
        let mut name_length = name.len() as u32;
        let mut domain_length = domain.len() as u32;
        let mut use_: SID_NAME_USE = 0;
        // SAFETY: two live buffers whose capacities are passed.
        let resolved = unsafe {
            LookupAccountSidW(
                std::ptr::null(),
                sid,
                name.as_mut_ptr(),
                &mut name_length,
                domain.as_mut_ptr(),
                &mut domain_length,
                &mut use_,
            )
        } != 0;

        Field::available(ProcessOwner {
            id,
            name: resolved.then(|| account_name(&wide_to_string(&domain), &wide_to_string(&name))),
        })
    }

    fn probe(pid: u32, access: Access) -> Capability {
        match pin_handle(pid, access) {
            Ok(_) => Capability::allowed(),
            Err(ControlError::PermissionDenied(_)) => Capability::denied(
                "Permission denied: Windows refused the rights this action needs, and PULSE does \
                 not elevate.",
            ),
            Err(ControlError::Gone) => Capability::denied("Process already exited."),
            Err(_) => Capability::denied("Windows could not open the process."),
        }
    }

    fn system_root() -> Option<String> {
        std::env::var("SystemRoot").ok()
    }

    fn executable_info(path: &str) -> ExecutableInfo {
        let metadata = std::fs::metadata(path);
        ExecutableInfo {
            path: path.to_string(),
            file_name: path.rsplit(['\\', '/']).next().unwrap_or(path).to_string(),
            size_bytes: match &metadata {
                Ok(metadata) => Field::available(metadata.len()),
                Err(_) => Field::missing(Availability::temporarily_unavailable(
                    "the executable file could not be read",
                )),
            },
            modified_at: match metadata
                .ok()
                .and_then(|metadata| metadata.modified().ok())
                .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            {
                Some(duration) => Field::available(duration.as_millis() as u64),
                None => Field::missing(Availability::not_detected(
                    "the file reports no modification time",
                )),
            },
            replaced_on_disk: false,
        }
    }

    impl ProcessInspectorBackend for WindowsProcessInspector {
        fn inspect(&self, pid: u32) -> Result<RawDetails, ControlError> {
            let (parent, name) = inventory(pid).ok_or(ControlError::Gone)?;
            let (handle, token) = pin_handle(pid, Access::Query)?;

            let path = image_path(&handle).ok();
            let owner = read_owner(&handle);
            let category = match (&owner.value, pid) {
                (_, 0 | 4) => ProcessClass::SystemProcess,
                (Some(owner), _) => category_from_sid(&owner.id),
                (None, _) => classify::classify(pid, path.as_deref(), system_root().as_deref()),
            };

            let executable = match &path {
                Some(path) => Field::available(executable_info(path)),
                None => Field::missing(Availability::permission_denied(
                    "Windows would not name this process's executable",
                )),
            };
            let version_info = match &path {
                Some(path) => version::read(path),
                None => Field::missing(Availability::permission_denied(
                    "the executable could not be located",
                )),
            };

            let priority = match read_priority(&handle) {
                Ok(priority) => Field::available(priority),
                Err(_) => Field::missing(Availability::permission_denied(
                    "the priority class could not be read",
                )),
            };
            let affinity = match read_affinity(&handle) {
                Ok(affinity) => Field::available(affinity),
                Err(_) => Field::missing(Availability::permission_denied(
                    "the CPU affinity could not be read",
                )),
            };

            let set_information = probe(pid, Access::SetPriority);
            let multi_group = affinity
                .value
                .as_ref()
                .is_some_and(|affinity| affinity.limitation.is_some());

            Ok(RawDetails {
                start_token: token,
                parent_pid: Some(parent).filter(|parent| *parent != 0),
                parent_name: parent_name(parent, token),
                name,
                state: ProcessState::Other,
                state_availability: Availability::unsupported(STATE_UNSUPPORTED),
                category,
                started_at: match filetime_to_unix_ms(token) {
                    Some(ms) => Field::available(ms),
                    None => Field::missing(Availability::not_detected(
                        "the creation time is not a date",
                    )),
                },
                executable,
                owner,
                architecture: read_architecture(&handle),
                priority,
                affinity,
                version_info,
                access: PlatformAccess {
                    terminate: probe(pid, Access::Terminate),
                    suspend: Capability::allowed(),
                    set_priority: set_information.clone(),
                    set_affinity: if multi_group {
                        Capability::denied(MULTI_GROUP_LIMITATION)
                    } else {
                        set_information
                    },
                },
            })
        }

        fn provenance(&self, pid: u32) -> Result<(u64, Provenance), ControlError> {
            let (token, path) = self.executable_path(pid)?;
            Ok((
                token,
                Provenance::Signature {
                    signature: authenticode::verify(&path),
                },
            ))
        }

        fn open_executable(&self, pid: u32) -> Result<OpenedExecutable, ControlError> {
            let (handle, token) = pin_handle(pid, Access::Query)?;
            let path = image_path(&handle)?;
            let file = std::fs::File::open(&path).map_err(|error| match error.kind() {
                std::io::ErrorKind::PermissionDenied => ControlError::PermissionDenied(
                    "Windows refused to open the executable for reading.".to_string(),
                ),
                _ => ControlError::Platform(format!("Opening the executable failed: {error}")),
            })?;
            // The handle held above pinned the PID for the whole open.
            drop(handle);
            Ok(OpenedExecutable {
                file,
                start_token: token,
            })
        }

        fn executable_path(&self, pid: u32) -> Result<(u64, String), ControlError> {
            let (handle, token) = pin_handle(pid, Access::Query)?;
            Ok((token, image_path(&handle)?))
        }
    }
}
