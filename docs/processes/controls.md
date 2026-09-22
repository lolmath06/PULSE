# Process controls

PULSE can suspend, resume, end, reprioritise and re-pin processes — **only when
the user explicitly asks**, and only the exact process instance selected.
Nothing is ever triggered by a metric, a threshold, a heuristic or a timer.
There is no automatic kill, suspend, priority tuning, affinity change or
"remediation".

## PID reuse safety

Every command names a `ProcessInstanceId` — PID **and** start token — parsed
strictly (`process:<pid>-<token>`, PID ≠ 0). Immediately before acting, the
service _pins_ whatever holds that PID and compares its start token:

```text
selected        PID 5000, start A
A exits, PID 5000 recycled by B
End process  →  pin(5000) reports start B  →  staleProcess, B untouched
```

Pinning closes the gap between the check and the action:

- **Linux** — `pidfd_open` first, then `/proc/<pid>/stat` for the token, then
  a check that the pidfd has not exited (so the token read was that process's).
  Signals go through `pidfd_send_signal`: if the process dies and its PID is
  recycled afterwards, the signal fails with `ESRCH` instead of reaching the
  newcomer. Kernels without pidfds (pre-5.3) fall back to re-reading the token
  immediately before `kill(2)`.
- **Windows** — `OpenProcess` first, creation time read _through that handle_.
  Windows does not recycle a PID while a handle to the process is open, so the
  check and the action refer to the same process. Handles are RAII-closed and
  never stored.

Tested at service level for every family (terminate, force kill, suspend,
resume, priority, affinity, reads): the fake backend records calls, and the
tests assert the newcomer received **none**.

## Results

`ProcessActionResult { status, reason, affectedCount, failedCount, tree }`:

`success` · `permissionDenied` · `staleProcess` · `processGone` ·
`unsupported` · `partialFailure` · `invalidRequest` · `platformError`

A refusal is never reported as "unsupported", and a process that already
exited is `processGone` — the outcome the user wanted, not a failure.

## Permissions

Actions run with the permissions of the user who started PULSE.
`EPERM`/`EACCES`/`ERROR_ACCESS_DENIED` → `permissionDenied`. No sudo, pkexec,
setuid helper, root daemon, UAC prompt or privilege enabling — ever.

Windows rights are the minimum per action:

| Action                          | Rights                                                                                 |
| ------------------------------- | -------------------------------------------------------------------------------------- |
| inspect, read priority/affinity | `PROCESS_QUERY_LIMITED_INFORMATION`                                                    |
| End process                     | `PROCESS_TERMINATE` + query-limited                                                    |
| priority, affinity              | `PROCESS_SET_INFORMATION` + query-limited                                              |
| suspend / resume                | query-limited; per thread `THREAD_SUSPEND_RESUME` + `THREAD_QUERY_LIMITED_INFORMATION` |

`PROCESS_ALL_ACCESS` is never requested.

## End process

| Platform | Action                                         |
| -------- | ---------------------------------------------- |
| Linux    | `SIGTERM` — the process may clean up or refuse |
| Windows  | `TerminateProcess` — immediate                 |

Never escalated automatically. **Force kill** (Linux only) is a separate,
explicitly named action sending `SIGKILL`, with a strong warning.

Confirmation dialogs name the process and PID; focus starts on **Cancel** and
nothing is sent before the confirm button. Ending PULSE itself stays possible
with the warning "Ending PULSE will close this application."

## End process tree

1. Pin and validate the root (stale root → nothing is ended).
2. Take one fresh snapshot of PID / parent PID / start token.
3. Plan descendants: a node is a child only if its parent PID matches **and** it
   started no earlier than that parent (Windows never rewrites parent PIDs; this
   stops a recycled parent PID from adopting strangers). Cycle-safe, bounded to
   4 096 nodes.
4. End deepest first, root last, **re-validating every member** immediately
   before its signal.
5. One bounded pass — processes spawned after the snapshot are not chased.

Summary: `requested`, `terminated`, `alreadyGone`, `permissionDenied`,
`staleSkipped`, `skippedSelf`, `failed` (always sums to `requested`). PULSE is
never ended as a side effect of a tree. The confirmation shows the root and the
approximate number of descendants from the current snapshot.

## Suspend / Resume

**Linux** — `SIGSTOP` / `SIGCONT`.

**Windows** — there is no documented "suspend process"; the undocumented
`NtSuspendProcess` is deliberately not used. PULSE takes a
`CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD)` snapshot, walks it with
`Thread32First`/`Thread32Next` (the thread walk has no `W` variant), opens each
thread owned by the target (checking `GetProcessIdOfThread` while the process
handle pins the PID) and calls `SuspendThread`, recording each thread ID it
added a suspension to. _Resume_ calls `ResumeThread` exactly once on those
threads and no others. Threads created after the suspension keep running —
inherent to the documented API. Partial refusals are `partialFailure` with counts.

**Ledger.** PULSE remembers which instances _it_ suspended this session.
_Resume_ is offered only for those. A process already stopped by something else
(a shell's Ctrl+Z, a debugger) is neither suspended again nor claimed: PULSE will
not undo someone else's stop. A suspended instance that exits or whose PID is
recycled is forgotten. PULSE never offers to suspend itself. PULSE does not
automatically resume on exit — a process suspended when PULSE closes stays
suspended.

## Priority

**Linux** — `getpriority` / `setpriority(PRIO_PROCESS)`. The real nice value
(−20…19) is always shown. Presets:

| Preset       | nice |
| ------------ | ---- |
| High         | −10  |
| Above normal | −5   |
| Normal       | 0    |
| Below normal | 5    |
| Low          | 19   |

plus _Custom nice value…_. Nice values are per **thread** on Linux; `renice -p`
changes only the main thread. PULSE applies the value to every thread in
`/proc/<pid>/task` (main thread first — its refusal is the result; worker
refusals are counted as `partialFailure`). Lowering nice below its current value
needs `CAP_SYS_NICE`/`RLIMIT_NICE`, so it is usually `permissionDenied`.

**Windows** — `GetPriorityClass` / `SetPriorityClass`: Idle (shown _Low_),
Below normal, Normal, Above normal, High, Realtime. **Realtime** needs a
specific confirmation ("Realtime priority can make the system unresponsive."),
enforced in the backend as well (`confirmRealtime`). Without the privilege
Windows silently applies High instead; PULSE reads the class back and reports
that as `permissionDenied` rather than claiming success.

## Affinity

**Linux** — `sched_getaffinity` / `sched_setaffinity`, applied to every thread
like nice values, restricted to online CPUs, read back afterwards.

**Windows** — `GetProcessAffinityMask` / `SetProcessAffinityMask` (subset of the
system mask). On machines with more than one processor group (> 64 logical
processors) PULSE shows the primary group and **does not change** affinity rather
than apply a partial mask; CPU Sets are not implemented.

The dialog has one checkbox per CPU and _Select all_; there is no _Clear all_,
and _Apply_ is disabled while no CPU is selected.

## After an action

One explicit snapshot refresh and one inspector re-read. No polling. If the
process has gone, the drawer says _Process exited_.
