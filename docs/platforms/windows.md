# PULSE on Windows

> **Current status — Phase 13C:** Windows is a physically validated,
> first-class PULSE platform. Native Windows CI, MSRV checks and packaging pass
> on the canonical Phase 13 commit. Physical Windows testing exercised live
> metrics, overlays, language switching, process inspection/control,
> persistence, lifecycle behaviour and the final portable/NSIS/MSI package.
>
> The historical sections below retain the implementation context of the phase
> in which each feature was introduced. Items such as the full DPI matrix,
> multi-monitor persistence, borderless-game overlays and the exhaustive
> process-inspector matrix were not separately exercised during Phase 13B.

Windows is a **first-class PULSE platform**, on equal footing with Fedora Linux.

## What PULSE reads on Windows today

**Implemented (Phases 2–6).** No administrator rights required.

| Metric                                                                  | API                                                                    | Notes                                                                  |
| ----------------------------------------------------------------------- | ---------------------------------------------------------------------- | ---------------------------------------------------------------------- |
| `cpu.usage.total`                                                       | `GetSystemTimes`                                                       | Delta between two samples                                              |
| `cpu.usage.logical`                                                     | `NtQuerySystemInformationEx` / `SystemProcessorPerformanceInformation` | One call per processor group                                           |
| `cpu.frequency.current`                                                 | `CallNtPowerInformation(ProcessorInformation)`                         | `CurrentMhz`, MHz → Hz                                                 |
| `cpu.frequency.max`                                                     | `CallNtPowerInformation(ProcessorInformation)`                         | `MaxMhz`, typically the **base** clock                                 |
| `cpu.count.logical`                                                     | `GetLogicalProcessorInformationEx`                                     | Set bits across core records                                           |
| `cpu.count.physical`                                                    | `GetLogicalProcessorInformationEx`                                     | `RelationProcessorCore` **records**                                    |
| `cpu.count.package`                                                     | `GetLogicalProcessorInformationEx`                                     | `RelationProcessorPackage` records                                     |
| `memory.total`                                                          | `GlobalMemoryStatusEx`                                                 | `ullTotalPhys`                                                         |
| `memory.available`                                                      | `GlobalMemoryStatusEx`                                                 | `ullAvailPhys`                                                         |
| `memory.used`                                                           | derived                                                                | `total - available`                                                    |
| `memory.usage.percent`                                                  | derived                                                                | `used / total * 100`                                                   |
| `gpu.count`                                                             | `CreateDXGIFactory1` + `EnumAdapters1`                                 | Hardware adapters; WARP excluded                                       |
| `gpu.memory.total`                                                      | `DXGI_ADAPTER_DESC1.DedicatedVideoMemory`, or NVML                     | Installed VRAM capacity                                                |
| `gpu.usage.core`, `gpu.memory.used`/`free`/`percent`, `gpu.frequency.*` | NVML only                                                              | `unsupported` without the NVIDIA driver                                |
| `gpu.temperature.core`, `gpu.fan.speed`                                 | NVML only                                                              | RPM only; a fan duty cycle is never republished as a speed             |
| `gpu.temperature.hotspot`, `gpu.temperature.memory`                     | —                                                                      | `unsupported`: NVML documents no source, and they are separate sensors |
| `cpu.temperature.package`                                               | —                                                                      | **`unsupported`**: declared, never measured — see below                |
| `storage.device.count`                                                  | SetupAPI `GUID_DEVINTERFACE_DISK`                                      | Present disk interfaces, not a `PhysicalDriveN` loop                   |
| `storage.volume.count`                                                  | `FindFirstVolumeW` / `FindNextVolumeW`                                 | Volume GUID paths, not drive letters                                   |
| `storage.capacity.total`                                                | `IOCTL_DISK_GET_DRIVE_GEOMETRY_EX`                                     | `DiskSize`, not the reported CHS geometry                              |
| `storage.io.*`                                                          | `IOCTL_DISK_PERFORMANCE`                                               | Per device, by handle; 100 ns → ms; delta between two samples          |
| `storage.health.*`                                                      | `IOCTL_STORAGE_QUERY_PROPERTY` + `ProtocolTypeNvme` log `0x02`         | NVMe only; ATA SMART deferred                                          |
| `storage.volume.capacity.*`                                             | `GetDiskFreeSpaceExW`                                                  | `used = total - free`, `available = free to this caller`               |
| `network.interface.count`                                               | `GetIfTable2` / `MIB_IF_ROW2`                                          | Loopback excluded; one call returns identity, state and counters       |
| `network.receive.*`, `network.transmit.*`                               | the same table                                                         | Packets are `Ucast + NUcast`; discards are drops, not errors           |
| `network.link.*_speed`                                                  | `ReceiveLinkSpeed`, `TransmitLinkSpeed`                                | Already bits/s; `0` and `u64::MAX` both mean unknown                   |
| `network.wifi.*`                                                        | WLAN `realtime_connection_quality`                                     | No SSID, no BSSID, no location permission — see below                  |

### GPU

DXGI is the generic inventory — no privileges, no service, no vendor SDK, the
same enumeration every Direct3D application performs. _Microsoft Basic Render
Driver_ (WARP) is excluded by both its `DXGI_ADAPTER_FLAG_SOFTWARE` flag and the
reserved `0x1414:0x008C` vendor/device pair, since older drivers do not always
set the flag. DXCore was considered and rejected: newer and richer, but aimed at
compute-adapter enumeration and unavailable on older supported Windows versions.

**`IDXGIAdapter3::QueryVideoMemoryInfo` is deliberately not published as VRAM
usage.** Its `CurrentUsage` is the video memory attributed to _the querying
process_, so PULSE would be reporting its own consumption and labelling it
system-wide — near zero on an idle machine while a game filled the card. A
DXGI-only adapter therefore publishes the installed capacity and nothing else;
`—` beats a number that is confidently wrong.

**Correlation with NVML** is by **PCI bus address**, not by enumeration order.
DXGI does not report one, so PULSE asks the kernel graphics subsystem for it:
`D3DKMTOpenAdapterFromLuid` with the adapter's `AdapterLuid`, then
`D3DKMTQueryAdapterInfo(KMTQAITYPE_ADAPTERADDRESS)`, then `D3DKMTCloseAdapter` on
every path. The entry points are `gdi32` exports of the kernel-mode graphics
interface, so they are resolved at runtime like `NtQuerySystemInformationEx`;
their absence costs one correlation hint, never the application.

Where no address is available, PULSE pairs an adapter with an NVML device **only
when exactly one of each is left unmatched**. Two NVIDIA cards are never paired
by enumeration order: the two APIs enumerate independently, and a wrong pairing
writes the wrong card's identity into a saved dashboard. See
`docs/metrics/gpu.md` for the full rules, including what happens to an adapter
that could not be paired.

**Identity** is the honest weak point of DXGI. It exposes no PCI bus address and
no Plug-and-Play device instance ID. `AdapterLuid` is documented as valid only
until restart, so PULSE never stores it — a resolved bus address is correlation
data, and does not become the identity, so upgrading to a build that can resolve
one never changes a `SourceId` a dashboard already holds. A Windows adapter is
identified by its
`vendor:device:subsystem:revision` tuple, with a session-scoped index appended
only when two adapters share it — recorded as `ModelWithSessionDisambiguator`
so the weaker guarantee is inspectable rather than silent. **NVIDIA cards are
unaffected**: NVML supplies a hardware UUID, and the merge replaces the tuple
with it, so the same card carries the same `SourceId` on Windows and on Fedora.

NVML is looked for in exactly two places, in order:

1. `%SystemRoot%\System32\nvml.dll`, via
   `LoadLibraryExW(L"nvml.dll", NULL, LOAD_LIBRARY_SEARCH_SYSTEM32)` — where the
   display driver installs it;
2. `<Program Files>\NVIDIA Corporation\NVSMI\nvml.dll`, by absolute path, via
   `LoadLibraryExW(path, NULL, LOAD_LIBRARY_SEARCH_SYSTEM32 | LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR)`
   — the layout NVIDIA's management tooling uses, and the copy present on
   machines whose driver package left none in the system directory.

Program Files is located with `SHGetKnownFolderPath(FOLDERID_ProgramFiles)`, not
read from `%ProgramW6432%`: the environment is inherited, and anything that can
start PULSE can set it.

The flags matter: a plain `LoadLibraryW` searches the application directory
first, so anyone able to drop a file beside `pulse.exe` could have PULSE load
their DLL — a classic DLL planting vulnerability. PULSE never widens the search
beyond those two locations — not the application directory, not the working
directory, not `PATH`.

### Why `NtQuerySystemInformationEx` for per-processor usage

PDH's `\Processor Information(*)\% Processor Time` is the obvious candidate and
was rejected: its counter paths are **localised** (`Processeur` on a French
install), it needs a registry-backed name lookup to avoid that, it carries a
query/counter handle lifecycle, and instance names must be string-parsed to be
attributed to a processor. WMI needs a service and costs tens of milliseconds.
`Get-Counter` and `typeperf` are subprocesses. `QueryIdleProcessorCycleTime`
returns cycles, which on a hybrid CPU are not comparable between P- and E-cores.

`SystemProcessorPerformanceInformation` returns a flat array of the exact
`IdleTime`/`KernelTime`/`UserTime` totals the shared delta model already
consumes, with no strings anywhere, in one call per processor group. The full
comparison is in [`../metrics/cpu-advanced.md`](../metrics/cpu-advanced.md).

### `NtQuerySystemInformationEx` is an optional capability

`ntdll` is documented by Microsoft as subject to change, and PULSE does **not**
present this entry point as a guaranteed part of Windows. It is therefore
**resolved at runtime** — `GetModuleHandleW("ntdll.dll")` followed by
`GetProcAddress`, plus a probe query — rather than imported at load time.

That distinction is the whole point. A load-time import is resolved by the
Windows loader before any PULSE code runs, so on a Windows that does not export
the symbol the process would fail to start outright. Resolving it at runtime
means its absence costs exactly one metric family:

```text
cpu.usage.total     available     GetSystemTimes            (kernel32)
cpu.count.*         available     GetLogicalProcessorInformationEx (kernel32)
cpu.frequency.*     available     CallNtPowerInformation    (powrprof)
memory.*            available     GlobalMemoryStatusEx      (kernel32)
cpu.usage.logical   unsupported   ← the only casualty
```

`GetModuleHandleW` is used rather than `LoadLibrary`: `ntdll.dll` is mapped into
every Win32 process before any user code runs, so the lookup touches no
filesystem and loads no arbitrary library.

Failure at any step — module, symbol, or probe — produces a structured
`MetricError` that becomes `Availability::Unsupported`. Nothing on that path
panics, and `WindowsCpuProvider` still constructs. The per-processor metrics
remain in the catalog with an honest status rather than being dropped, so a
saved dashboard keeps the same references.

Note that **`cpu.usage.total` is deliberately not derived** from this array even
when it is available: keeping the aggregate on the independent, fully documented
`GetSystemTimes` is what makes it survive this capability being absent.

Resolution happens once, at provider construction, and the resolved pointer is
reused for every sample — there is no `GetProcAddress` per refresh or per
processor, and no mutable global.

### Processor groups

Windows addresses logical processors as `(group, index in group)`, at most 64
per group. **The implementation is not capped at 64 processors**: the plain
`NtQuerySystemInformation` form would silently report only the calling thread's
group, so PULSE uses the `Ex` form once per group.

Since there is no system-wide flat processor number, PULSE assigns its own
ordinal by sorting every `(group, index)` pair — group first, then index — and
numbering from zero, giving `cpu:logical-0`…`cpu:logical-N`. On a single-group
machine this is the identity mapping, so `cpu:logical-5` is Task Manager's
_CPU 5_. The sort makes the mapping independent of the order Windows returned
records in, so it cannot shift between runs and re-point saved references.

### `MaxMhz` is usually the base clock

On most Windows systems `MaxMhz` reports the processor's **base** frequency
rather than its maximum turbo frequency — the same figure the System control
panel shows, and typically lower than Linux's `cpuinfo_max_freq` on the same
chip. It is a hardware figure rather than a power-policy ceiling (that is
`MhzLimit`, which PULSE does not publish), so it is published as
`cpu.frequency.max` with that meaning documented rather than quietly equated to
the Linux number.

Two Windows-specific details worth knowing:

- **`GetSystemTimes` counts idle time inside `KernelTime`.** So
  `total = kernel + user` and `busy = total - idle`. Applying the Linux formula
  here would report a completely idle machine as heavily busy. This is the one
  genuine trap in the phase, and it has a test named after it. The same applies
  to the per-processor counters, and a test asserts the two conversions agree.
- **Physical cores are `RelationProcessorCore` records, not set bits.**
  Counting the bits in the affinity masks counts hardware threads and would
  report an 8-core SMT machine as having 16 cores.
- **`dwMemoryLoad` is deliberately ignored.** Windows offers a ready-made
  percentage, but it is rounded to a whole number and would disagree with the
  `used` and `total` figures shown beside it — and with how Fedora reports the
  same thing. PULSE computes from the byte counts on both platforms.

A `FILETIME` is a 64-bit value split across two 32-bit fields, and Microsoft's
documentation is explicit that it must not be cast directly to a 64-bit
integer; PULSE recombines the halves explicitly.

Full formulas and edge cases: [`../metrics/cpu-memory.md`](../metrics/cpu-memory.md).

### Dependencies and `unsafe`

The only Windows crate used for metrics is `windows-sys` — raw FFI bindings,
no wrapper layer, no runtime — declared under
`[target.'cfg(target_os = "windows")'.dependencies]` with exactly the features
that declare the calls PULSE makes. `PROCESSOR_POWER_INFORMATION` is not shipped
by `windows-sys` and is declared next to the code that uses it; DXGI is COM,
which `windows-sys` deliberately does not cover, so the `windows` crate supplies
those bindings — it is already in the dependency graph on Windows because Tauri
pulls it in, so this buys correct COM lifetime handling at no build cost; DXGI is COM,
which `windows-sys` deliberately does not cover, so the `windows` crate supplies
those bindings — it is already in the dependency graph on Windows because Tauri
pulls it in, so this buys correct COM lifetime handling at no build cost;
`NtQuerySystemInformationEx` is declared as a **function-pointer type** rather
than an `extern` block, because it is resolved dynamically — a type alias
creates no import, which is precisely what keeps its absence from being fatal.
There is no `#[link]` attribute and no `extern "system" { … }` block anywhere in
PULSE.

Every `unsafe` block is a handful of lines wrapping one API call, each with a
`# Safety` comment explaining why it is sound, and each converting the packed
Windows buffer into plain Rust structs immediately. The dynamic resolution adds
three, all in `platform/windows/ntdll.rs`: the `GetModuleHandleW` lookup, the
`GetProcAddress` lookup, and the `transmute` of the returned `FARPROC` onto the
declared signature — after which the pointer is private to a safe wrapper and
never reaches the provider. Everything after that —
FILETIME recombination, the CPU counter semantics, the memory convention, the
processor-group ordinal mapping, core and package counting, MHz→Hz conversion —
is safe, pure Rust that compiles and is **unit-tested on Fedora**. That is why
128- and 256-processor multi-group layouts are covered by tests written on a
laptop that has neither.

No administrator rights are required by any of it.

## What Phase 0 already does on Windows

- Reports the OS version via the `windows-version` crate, distinguishing
  Windows 10 from Windows 11 by build number (both report major version 10;
  build ≥ 22000 means Windows 11).
- Reports no display server, since Windows has a single compositor (DWM) and
  there is nothing analogous to the Wayland/X11 split.
- Lives in `src-tauri/src/platform/windows/`, compiled only on Windows, with its
  dependencies declared under
  `[target.'cfg(target_os = "windows")'.dependencies]` so they can never affect
  a Fedora build.

## Development prerequisites

- **Microsoft C++ Build Tools** with the "Desktop development with C++"
  workload (MSVC toolchain and the Windows SDK).
- **WebView2 Runtime** — preinstalled on Windows 11 and on up-to-date
  Windows 10; otherwise install the Evergreen Bootstrapper.
- **Rust** via `rustup` (the `x86_64-pc-windows-msvc` toolchain).
- **Node.js 20.19+** and pnpm.

### Type-checking the Windows code from Fedora

The full application cannot be _built_ for Windows on a Fedora workstation:
`tauri-build` compiles a Windows resource file and needs a resource compiler
(`llvm-rc`) that a stock Fedora toolchain does not ship. Everything PULSE writes
itself is tauri-free, so `tools/windows-check/` includes the `metrics/` and
`platform/` module trees **by path** — the same files, never a copy — and type
checks them for a real Windows target:

```bash
rustup target add x86_64-pc-windows-msvc
pnpm rust:windows        # cargo check  --target x86_64-pc-windows-msvc
pnpm rust:windows:lint   # cargo clippy --target x86_64-pc-windows-msvc -D warnings
```

The harness declares the same `rust-version = 1.77.2` as the application, and
is itself checked with the 1.77.2 toolchain
(`cargo +1.77.2 check --manifest-path tools/windows-check/Cargo.toml --target x86_64-pc-windows-msvc`)
— see [`../development/msrv.md`](../development/msrv.md).

It proves the Windows CPU, memory, GPU, storage and network providers, the DXGI
and `D3DKMT` layers, the SetupAPI disk enumeration, the storage and volume
device controls, the NVMe health path, `GetIfTable2`, the WLAN realtime-quality
path, the NVML loader and every metric declaration compile and type check for
Windows. It proves **nothing about runtime
behaviour** — nothing here executes on Windows. CI's `windows-latest` job builds
and runs the whole crate; a physical machine is still the only real validation.

The harness is deliberately not the only guard. Everything in the Windows
storage layer that is _pure_ — the `STORAGE_DEVICE_DESCRIPTOR` string offsets,
the bus-type mapping, the NVMe log-page extraction, the `DISK_PERFORMANCE` unit
conversion, the disk-extent parsing, the identity rules — takes a byte slice or
a string and returns a value, so it is **unit-tested on Fedora** against
synthetic buffers. Getting a structure offset wrong is exactly the kind of bug
that would otherwise surface only on a machine PULSE has not run on yet.

## Planned data sources

None of this is implemented yet; it is the map for later phases.

### The main options, and their trade-offs

| API                                          | Good for                           | Cost                                                             |
| -------------------------------------------- | ---------------------------------- | ---------------------------------------------------------------- |
| **PDH** (Performance Data Helper)            | CPU, disk, network counters        | Cheap, stable, well documented                                   |
| **WMI**                                      | Inventory, hardware identification | Convenient but **slow**; unsuitable for a polling loop           |
| **Win32 APIs**                               | Memory, uptime, process list       | Cheapest, most direct                                            |
| **LibreHardwareMonitor-style ring-0 driver** | Motherboard sensors, fan RPM       | Requires a signed kernel driver — a large decision, not a detail |

The rule of thumb: **WMI for one-shot identification, PDH or Win32 for anything
sampled repeatedly.** Polling WMI in a loop is the standard way to make a
monitoring app cost more CPU than the things it measures.

### CPU

- `GetSystemTimes` for aggregate utilisation, as deltas — **implemented**.
- `NtQuerySystemInformation` for per-processor detail, a later phase.
- PDH `\Processor Information(*)\% Processor Utility` for per-core detail.
- Frequency is awkward: `\Processor Information(*)\% Processor Performance`
  scaled by the nominal frequency is the usual approach.

### Memory

- `GlobalMemoryStatusEx` for physical totals — **implemented**.
- `GetPerformanceInfo` for commit charge and page-file detail, a later phase.

### Storage — implemented in Phase 6

Not PDH. `\PhysicalDisk(0 C:)\Disk Reads/sec` is the obvious source and is not
used: its instance names are presentation strings built from a disk number and
whichever drive letters happen to sit on it, they change when a letter is
reassigned, they are localised on some systems, and a disk holding several
volumes produces a name PULSE would have to parse to attribute anything.
Correlating that back to a device descriptor would be a guess dressed as a
measurement.

What is used instead, all by handle and all read-only:

- **SetupAPI** over `GUID_DEVINTERFACE_DISK` for enumeration — not a loop over
  `\\.\PhysicalDrive0`, `1`, … until one fails, which is wrong in both
  directions: the numbers are not contiguous, and a number that fails for a
  permissions reason ends the loop early and hides every disk after it. SetupAPI
  also supplies the **device instance ID**, the stable Windows identifier.
- `IOCTL_STORAGE_QUERY_PROPERTY` for the device descriptor — vendor, product,
  revision, serial, bus type, removable media.
- `IOCTL_STORAGE_GET_DEVICE_NUMBER` to learn which `PhysicalDriveN` a handle is,
  which is how a volume's extents are matched to a disk. A _handle_, never an
  identity.
- `IOCTL_DISK_GET_DRIVE_GEOMETRY_EX` for capacity.
- `IOCTL_DISK_PERFORMANCE` for cumulative I/O counters.
- `FindFirstVolumeW` / `FindNextVolumeW` and
  `GetVolumePathNamesForVolumeNameW` for volumes and the paths they are
  reachable at. Enumerating drive letters instead would miss the EFI system
  partition, the recovery partition and any volume mounted into a folder.
- `IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS` to map a volume onto the disks it
  occupies — several, for a striped or spanned volume, which PULSE reads rather
  than assuming one.
- `GetDiskFreeSpaceExW` for filesystem usage.
- `IOCTL_STORAGE_QUERY_PROPERTY` with `StorageDeviceProtocolSpecificProperty`,
  `ProtocolTypeNvme` and `NVMeDataTypeLogPage` for the NVMe SMART / Health log.

**No `PowerShell`, `wmic`, `diskpart`, `fsutil`, `Get-PhysicalDisk` or
`Get-Disk`, anywhere.** Every handle is opened with `dwDesiredAccess = 0`: no
read access, no write access, only the informational controls above. Nothing in
PULSE can modify a disk.

### Network — implemented in Phase 7

Not PDH. `\Network Interface(*)\Bytes Received/sec` is the obvious source and
is not used, for the same reason `\PhysicalDisk` was not used for storage: its
instance names are presentation strings derived from the adapter description,
they are localised, they are mangled — parentheses and slashes are rewritten —
and correlating one back to a `MIB_IF_ROW2` means matching munged text.

What is used instead:

- **`GetIfTable2`** — one call returning every interface's LUID, GUID,
  description, alias, both hardware addresses, MTU, type, physical medium,
  operational status, media connect state, both link speeds **and** twenty
  counters. Freed with `FreeMibTable` through an RAII guard, and the rows are
  copied out before the table is released, so no raw pointer leaves the FFI.
- **`PermanentPhysicalAddress`** for identity, falling back to
  `InterfaceGuid` for a virtual adapter that has none. **Never
  `InterfaceIndex`**, which Microsoft documents as changing when adapters are
  added or removed.
- **`NDIS_PHYSICAL_MEDIUM`** to recognise a wireless adapter. Not `Type`: a
  great many wireless drivers report `IF_TYPE_ETHERNET` for compatibility,
  exactly as a Linux Wi-Fi station reports `ARPHRD_ETHER`.
- **The `HardwareInterface` flag** to tell a real Ethernet port from a Hyper-V
  switch or a VPN adapter, both of which report Ethernet without being
  hardware.
- **`WlanOpenHandle` / `WlanEnumInterfaces` / `WlanQueryInterface`** with
  `wlan_intf_opcode_realtime_connection_quality`. Buffers freed with
  `WlanFreeMemory`, through the same guard pattern.

`MIB_IF_ROW2` is declared in PULSE as a `#[repr(C)]` struct rather than taken
from a binding crate, with a compile-time assertion pinning its documented
1352-byte size. The compiler computes the offsets — there are no hand-counted
byte positions — and every field read is a pure function, so it is unit-tested
on Fedora against synthetic rows.

### Processes — implemented in Phase 8

**No `tasklist`, no `wmic`, no PowerShell, no `Get-Process`.** Each spawns a
process — PowerShell spawns a runtime — to format for a console what these
calls return as numbers, and on a machine with four hundred processes that is a
per-refresh cost measured in hundreds of milliseconds plus a text format to
parse back.

Two tiers, because protected processes exist:

| Tier         | API                                                                     | Yields                                                                                                 |
| ------------ | ----------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------ |
| Inventory    | `CreateToolhelp32Snapshot` + `Process32FirstW`/`Process32NextW`         | PID, parent PID, thread count, image name — no handle needed                                           |
| Per process  | `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION)`                        | a short-lived handle for everything below                                                              |
|              | `GetProcessTimes`                                                       | kernel + user CPU time, **and** the creation-time identity                                             |
|              | `K32GetProcessMemoryInfo`                                               | `WorkingSetSize`                                                                                       |
|              | `GetProcessIoCounters`                                                  | `ReadTransferCount` / `WriteTransferCount` (all read/write I/O, not block-level — see processes.md §6) |
|              | `QueryFullProcessImageNameW`                                            | the executable, for grouping and classification                                                        |
| Denominators | `GetActiveProcessorCount(ALL_PROCESSOR_GROUPS)`, `GlobalMemoryStatusEx` | the CPU and memory percentage divisors                                                                 |

`GetActiveProcessorCount(ALL_PROCESSOR_GROUPS)` rather than `GetSystemInfo`:
the latter reports only the calling thread's processor group and would say 64
on a 128-thread workstation, doubling every process's CPU percentage.

**Handles never outlive the call that opened them.** Every one is wrapped in a
guard that closes it on drop, including on early returns, and the cycle is
strictly enumerate → open → read → close. The rate baselines hold an identity
and two integers; keeping four hundred handles open across refreshes would pin
every one of those processes' kernel objects for as long as PULSE ran.

Two Windows facts PULSE reports rather than papers over:

- **There is no process-level state.** Windows schedules threads; a process is
  a container. `process.count.running` is declared `unsupported` with a reason
  rather than synthesised from thread states, because a synthesised figure
  would make the same column mean two different things on the two platforms.
- **A protected process refuses `OpenProcess`**, even to an administrator. The
  Toolhelp row survives with PID, parent, threads and name; CPU, memory, I/O
  and the executable path report `permissionDenied`. PULSE does not elevate.

`FILETIME` is recombined as `high << 32 | low` and converted from 100 ns
intervals to nanoseconds, so the delta arithmetic is shared with Linux clock
ticks. Reading `dwLowDateTime` alone — which looks plausible, because it is
usually the only half that changes — wraps every seven minutes of CPU time;
there is a test asserting the two halves genuinely differ.

See [`../metrics/processes.md`](../metrics/processes.md) for the CPU
normalisation, the PID-reuse identity and the application grouping.

### Process inspector and controls — implemented in Phase 9, physically exercised in Phase 13B

Compiled for `x86_64-pc-windows-msvc` (`pnpm rust:windows`,
`pnpm rust:windows:lint`) and its pure logic (trust mapping, version-resource
parsing, machine types, FILETIME → date, priority classes, affinity masks, SID
categories) is unit-tested on Fedora. **Phase 13B later exercised the inspector and process-control path on physical Windows using a disposable Notepad process.** The complete protocol below was not exhaustively rerun.

| Need                  | API                                                                                                                                                           |
| --------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| pin + identity        | `OpenProcess` (minimum rights) → `GetProcessTimes` on that handle, `GetExitCodeProcess`                                                                       |
| owner                 | `OpenProcessToken` → `GetTokenInformation(TokenUser)` → `LookupAccountSidW`, `ConvertSidToStringSidW`                                                         |
| architecture          | `IsWow64Process2` (resolved at run time), fallback `IsWow64Process` + `GetNativeSystemInfo`                                                                   |
| version resource      | `GetFileVersionInfoSizeW`, `GetFileVersionInfoW`, `VerQueryValueW`                                                                                            |
| signature + publisher | `WinVerifyTrust` (embedded, then catalog via `CryptCATAdmin*`), `WTHelper*`, `CertGetNameStringW` — revocation off, cache-only                                |
| end                   | `TerminateProcess` (`PROCESS_TERMINATE`)                                                                                                                      |
| suspend / resume      | `CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD)`, `Thread32First/Next`, `OpenThread`, `SuspendThread`, `ResumeThread` — only threads PULSE suspended are resumed |
| priority              | `GetPriorityClass`, `SetPriorityClass` (read back: Realtime without privilege becomes High and is reported)                                                   |
| affinity              | `GetProcessAffinityMask`, `SetProcessAffinityMask`; > 64 logical processors: read-only, documented                                                            |

No PowerShell, no `signtool`, no WMI, no `NtSuspendProcess`, no
`PROCESS_ALL_ACCESS`, no elevation. See `docs/processes/`.

#### Historical manual test protocol — partially exercised in Phase 13B

To run on a real Windows 10/11 machine, comparing against Task Manager:

1. Inspector on `explorer.exe`, `notepad.exe`, a browser, a third-party app:
   path, owner, start time, architecture, priority, affinity vs Task Manager's
   _Details_ tab.
2. Signature: `notepad.exe` (catalog-signed → _Trusted_, source catalog), a
   signed third-party app (_Trusted_, publisher), an unsigned build
   (_Unsigned_). No network traffic while verifying (e.g. Resource Monitor).
3. Version resource: product/company/version match the file's _Properties →
   Details_.
4. SHA-256 vs `certutil -hashfile <exe> SHA256`.
5. Search online / Search hash online / VirusTotal: browser opens; URL has no
   `C:\Users\…`, no user name, no PID.
6. Open file location: Explorer opens with the file selected.
7. On a `ping -t localhost` or `notepad` started for the test **only**:
   Suspend (Task Manager shows _Suspended_), Resume, End process, End process
   tree (a `cmd` → child `cmd` → `ping` tree), priority Below normal / High,
   Realtime confirmation (and the High fallback without privilege), affinity.
8. Protected process (`csrss.exe`, an anti-malware service): actions disabled
   or `permissionDenied`, never an elevation prompt.
9. PID reuse: note an instance ID, end that test process, start new ones until
   the PID is reused if feasible, act from the stale inspector → _staleProcess_.
10. Context menu (right click, Shift+F10, Escape), sorting toggle on every
    column in both views, Refresh keeps the sort.
11. Close PULSE; relaunch; no PULSE process left in Task Manager.

### Wi-Fi without location permission

The obvious source for a signal strength is
`wlan_intf_opcode_current_connection`, which returns signal quality, rates,
**and the SSID and BSSID**. On recent Windows those last two make the call
subject to the machine's **location permission**, because a BSSID is a
geolocation primitive.

PULSE wants _how good is this link_, not _which network is this and where_, so
it asks the realtime-quality opcode, which carries no network identity at all.
Nothing in the Windows network layer requests, parses, stores or displays an
SSID or a BSSID.

That opcode is recent. On a Windows build that does not implement it, the query
fails and the four Wi-Fi metrics report as unavailable with that reason —
PULSE does **not** fall back to the location-gated call. Falling back would
work, and would mean a monitoring tool silently reaching for a location-gated
interface behind the user's back. The generic interface metrics are unaffected
either way, which is the point of keeping Wi-Fi as a capability rather than a
precondition.

### CPU temperature — declared, and honestly unsupported

**Decided in Phase 5: `cpu.temperature.package` is `unsupported` on Windows.**

Windows has no general, unprivileged, hardware-independent temperature API.

- `MSAcpi_ThermalZoneTemperature` (WMI) and `Win32_TemperatureProbe` exist, are
  frequently unimplemented by OEM firmware, and report an **ACPI thermal zone** —
  which may describe the chassis, the mainboard or a platform zone rather than
  the processor. Publishing whichever one answers as _the CPU's_ temperature
  would be a confident, unfalsifiable lie, so PULSE publishes neither.
- Reading the actual package means reading MSRs; real motherboard sensors mean
  talking to a Super I/O chip. Both need ring-0 access, i.e. a signed kernel
  driver. HWiNFO, LibreHardwareMonitor, OpenHardwareMonitor and `WinRing0` ship
  exactly that. **PULSE bundles none of them**, in this phase or as a hidden
  dependency: signing costs, security exposure and support burden are a decision
  of their own, not a detail of a sensors phase.

The metric is nonetheless **declared**, on `cpu:package-N` sources, with an
`unsupported` availability carrying that explanation. That is what lets a
dashboard configured on Fedora open on Windows and explain itself instead of
silently losing a widget — and the day PULSE gains a supported source, the
reference it is already publishing starts carrying values.

The package index is the enumeration ordinal from
`GetLogicalProcessorInformationEx`, which is all Windows offers. Nothing is read
from it, so nothing can be attributed to the wrong socket.

Open for a later phase, unchanged: integrating with an existing tool's
opt-in shared-memory interface (HWiNFO's, for instance), or shipping a driver.
Neither is done quietly.

### GPU

- **NVIDIA** — NVML, the most complete source. Temperature and RPM fan speed are
  **implemented**; hotspot and memory temperatures are unsupported because the
  public interface documents no source for them.
- **AMD** — ADLX / ADL. Not integrated: a large vendor SDK for a temperature is
  not a trade this phase makes, so AMD GPU thermals are `unsupported` on Windows
  while being fully supported on Fedora through `amdgpu`'s hwmon node.
- **Intel** — Intel Power Gadget's successors, or DXGI. Likewise unsupported.
- **Vendor-neutral baseline** — DXGI adapter info plus the
  `\GPU Engine(*)\Utilization Percentage` PDH counter set, which needs no vendor
  SDK and works for basic utilisation and VRAM.

## Privileges

Same principle as Fedora: **PULSE must be useful without administrator
rights.** Elevation-requiring data (MSR-based temperatures, some PDH counter
sets) is an optional, clearly labelled enhancement.

`GetIfTable2` and the WLAN queries PULSE uses need no elevation. Where the WLAN
service is stopped, the machine has no wireless hardware, or the Windows build
predates the realtime-quality opcode, the Wi-Fi metrics report as unavailable
and every generic network metric keeps working.

The storage layer is designed around this: `dwDesiredAccess = 0` is what lets an
unelevated process issue the device queries at all. Where Windows still refuses
— a driver that will not answer the NVMe log page without elevation is the
expected case — the affected metrics report `permissionDenied`, never
`unsupported`, so the user is told the OS refused rather than that their drive
has no health data.

`IOCTL_DISK_PERFORMANCE` depends on the disk performance counters being
collected. They are enabled by default for physical disks on every supported
Windows version; where they are not, the control fails and the six
`storage.io.*` metrics report `unsupported` rather than being invented.

Processes follow the same rule with a sharper edge: the Toolhelp inventory
needs no rights at all, so **no process ever disappears from the list**, and
the four values a handle would have supplied degrade to `permissionDenied` on
that row alone. PULSE never requests elevation to widen this.

## Windows-specific behaviours to design for

- **Per-monitor DPI awareness** — mixed-DPI multi-monitor setups are common and
  will matter for the Mini overlay's positioning.
- **Exclusive fullscreen** — an always-on-top overlay is generally not composited
  over an exclusive-fullscreen surface. Borderless windowed is the realistic
  target for Gaming mode.
- **SmartScreen** — unsigned installers trigger a warning. Code signing is a
  release-phase decision.
- **Path handling** — never assume `/`; use `std::path` throughout, and never
  hardcode `C:\`.

## History and visualization (Phase 10)

- **Database:** `%LOCALAPPDATA%\dev.pulse.app\history.sqlite3` (Tauri's
  `app_local_data_dir`; local, not roaming).
- **Compiled and later physically exercised.** `tools/windows-check` includes the real
  `history/` module — SQLite store, migrations, queries, retention, scheduler,
  database-path function — and `services/history.rs`, checked and linted for
  `x86_64-pc-windows-msvc`. There is no Windows stub.
- **Status from Fedora, precisely:**

  | What                                             | Status                                                    |
  | ------------------------------------------------ | --------------------------------------------------------- |
  | Windows Rust / history integration               | **Cross-checked** (`check` + `clippy`, stable and 1.77.2) |
  | Bundled SQLite C amalgamation compiled with MSVC | **Verified by native Windows CI**                         |
  | Windows physical runtime                         | **Verified in Phase 13B**                                 |

  The application enables `rusqlite`'s `bundled` feature (rusqlite 0.32.1,
  SQLite 3.46.0), which compiles SQLite's C code with MSVC (`cl.exe`,
  `lib.exe`). Those tools do not exist on a Fedora host and no system package may
  be installed, so the **harness alone** uses `rusqlite` without `bundled`
  (named as a known limitation in `tools/windows-check/Cargo.toml`). Every line
  of PULSE's Rust that talks to SQLite, plus `rusqlite` and `libsqlite3-sys`, is
  type checked for Windows; the C compilation is not. Production Windows still
  uses the bundled SQLite. Native Windows CI now builds and tests the real production configuration,
  including bundled SQLite, and the final Phase 13 CI is green.

### Historical manual test protocol — partially exercised in Phase 13B

Phase 13B physically exercised history recording/persistence and relaunch behaviour. The broader visual and workload matrix below remains a historical compatibility protocol and was not run exhaustively.

1. `pnpm app:dev`. The History recorder card shows _Recording · Every 5 s_ and a
   path under `%LOCALAPPDATA%\dev.pulse.app\`. The file exists there.
2. Wait 30 s. _Batches this session_ rises by about one every 5 s; _Database
   details_ shows rows growing and journal `WAL`.
3. **CPU Total:** the CPU history chart shows a percentage and a curve close to
   Task Manager's overall CPU. Switch to _Logical processors_: small sparklines.
4. **Presets:** Customize → Clean, Minimal, Technical, Gaming, Compact — each
   visibly different; pick a custom primary colour with the colour picker.
5. **Small sparkline:** Compact preset, Size Custom, height 36 — readable, no
   axes, no clipping. **Large:** Size Large, window maximised — no pixelation.
6. **Storage / network:** copy a large file and open a web page; Read/Write and
   Download/Upload react within ~10 s.
7. **Refresh:** click Refresh on several cards repeatedly; in _Database details_
   the batch count still grows by one per 5 s only.
8. **Restart:** choose Area + custom colour + grid off, close PULSE, check Task
   Manager shows no `pulse.exe`, relaunch: style, range and earlier samples
   return, with a gap for the closed period; new points continue.
9. **Shutdown:** after closing, `history.sqlite3-wal` is 0 bytes or absent.
10. Machine without a readable CPU temperature: the thermal panel lists it as
    _Not charted_ with the reason — never a flat 0 °C line.

## Dashboard, widgets and overlays (Phase 11)

- **Configuration:** `%APPDATA%\dev.pulse.app\ui-config.json`, atomic
  replace via `MoveFileExW(MOVEFILE_REPLACE_EXISTING)` (Rust `fs::rename`).
- **Cross-checked from Fedora** (`pnpm rust:windows`, `rust:windows:lint`,
  and with Rust 1.77.2): the config store, the live feed, and the overlay core
  — capabilities, monitor/DPI geometry, overlay specs, settings and shortcut
  conflict logic.
- **Native Windows CI now covers the full Tauri desktop layer**, including
  `set_ignore_cursor_events`, always-on-top, tray and the global-shortcut
  plugin. Phase 13B also exercised the corresponding overlay/lifecycle
  behaviour on physical Windows.
- **Physically verified in Phase 13B:** always-on-top, click-through,
  `RegisterHotKey` shortcut behaviour and tray/lifecycle operation.
  Per-monitor/DPI edge cases were not separately exercised.

### Native overlay backend (Phase 12)

Overlays now use PULSE's Windows native backend: tao's flags plus a window
subclass that enforces `WS_EX_TOOLWINDOW`, `WS_EX_NOACTIVATE`,
`WS_EX_LAYERED | WS_EX_TRANSPARENT` (locked) and `WS_EX_TOPMOST`, re-asserts
`HWND_TOPMOST` without activating, and reads the style back. The Win32 part
(`platform/windows/overlay_window.rs`) is type-checked by the harness;
**implemented, CI-verified and physically verified for the core Phase 13B behaviours** — see
[`../overlay/windows-native.md`](../overlay/windows-native.md) and its manual
checks.

### Historical manual test protocol — Phase 11 · partially exercised in Phase 13B

Phase 13B physically exercised the core overlay, shortcut, persistence and lifecycle paths. The full matrix below was not executed item-for-item.

1. **Dashboard persistence:** Dashboard → Edit layout → add CPU Total,
   Memory, CPU temperature, Network; move and resize; close PULSE; check Task
   Manager has no `pulse.exe`; relaunch — the layout returns;
   `%APPDATA%\dev.pulse.app\ui-config.json` exists.
2. **Overlay window:** Overlays → _Tiny stats_. A separate frameless window
   appears, not in the taskbar.
3. **Always-on-top:** click a normal window (Explorer, Notepad) over the
   overlay's position — the overlay stays above.
4. **Transparency:** set the background opacity to 0 — only the text shows.
5. **Click-through:** Lock the overlay over a browser window and click on it.
   Success = the browser receives the click **and** the focus, PULSE does not
   become focused, the overlay stays visible. Failure = the click is
   swallowed, PULSE takes focus, or the overlay goes behind / disappears.
6. **Global shortcut:** with **another** application focused, Ctrl+Shift+F12
   toggles Edit/Locked. (Windows keeps the plugin backend, `RegisterHotKey`;
   the Wayland portal backend is Linux-only and never used here.) Set a shortcut
   already used by another program — PULSE reports the conflict and keeps the
   old one.
7. **Tray:** Open PULSE, Edit overlays, Lock overlays, Show/hide, Quit.
8. **DPI:** at 100 %, 125 %, 150 % and 200 % display scaling, the overlay
   keeps its size and position after a restart.
9. **Multi-monitor:** move an overlay to a second monitor, restart — it
   returns there; unplug that monitor, restart — it appears on the primary one.
10. **Keep running:** set _Keep running_, show one overlay, close the main
    window — the overlay stays and keeps updating, `pulse.exe` is still
    running; _Open PULSE_ on the overlay (Edit mode) shows the same main
    window (no second one); then tray → Quit — no `pulse.exe` left.
11. **Minecraft (optional):** windowed and borderless — the locked overlay
    stays on top, clicks reach the game, focus never moves. Exclusive
    fullscreen is not supported.
12. **Clean quit:** tray → Quit; no `pulse.exe` and no `WebView2` child left;
    `history.sqlite3-wal` is 0 bytes or absent.

## Packaging

NSIS (`.exe`) and MSI, built natively by CI on `windows-latest`
(Phase 13A, [`../release/windows-ci.md`](../release/windows-ci.md)). The NSIS
installer installs for the current user — PULSE needs no administrator
rights, so its installer asks for none.

MSI requires a strictly numeric version while PULSE is `0.1.0-dev`: the MSI's
own version is set explicitly (`bundle.windows.wix.version = "0.1.0"`) and the
WiX upgrade code is pinned, so the app version keeps its pre-release suffix.
Binaries are unsigned for now (SmartScreen may warn).

## Testing note

Phase 13 closes the earlier Windows validation gap.

The final canonical state is covered by native Windows CI and by physical
Windows runtime testing. The final CI-produced portable executable, NSIS
installer and MSI were also tested on physical Windows and their SHA-256
checksums were verified.

This does not mean every supported Windows hardware configuration has been
physically exercised. DPI matrices, multi-monitor edge cases, games and the
full process-inspector compatibility matrix remain useful future regression
tests.

The authoritative closure record is
[`../release/windows-physical-validation.md`](../release/windows-physical-validation.md).
