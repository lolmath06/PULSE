# Process Inspector

The inspector answers, for **one** selected process: what is it, where does
its executable come from, who runs it, since when, and what can be done to it.

It opens as a drawer on the right when a row of the **Processes** table is
clicked (or Enter/Space on a focused row). The table stays visible and usable
beside it. Escape or the × closes it.

## Three services, three lifetimes

| Service                   | Reads / does                                   | When                         | Cost                         |
| ------------------------- | ---------------------------------------------- | ---------------------------- | ---------------------------- |
| `ProcessSnapshotService`  | the whole table                                | every _Refresh_              | ~10–25 ms for ~700 processes |
| `ProcessInspectorService` | one process in depth, read-only                | a row is selected            | a few syscalls               |
| `ProcessControlService`   | terminate, suspend, resume, priority, affinity | an explicit click, confirmed | one action                   |

The snapshot never computes anything the inspector shows: no hash, no
package, no signature, no version resource, no owner name. None of it is ever
computed on Refresh. The inspector and control services are not metric
providers and never enter the metric catalog (provider count stays 6).

## What is shown

| Section               | Fields                                                                                                                              |
| --------------------- | ----------------------------------------------------------------------------------------------------------------------------------- |
| Identity              | process name, product name (when it differs), PID, parent name + PID, instance ID, state, start time, owner, category, architecture |
| Resource usage        | CPU, memory, memory %, threads, read, write — from the latest snapshot row                                                          |
| Executable            | path, file name, size, last modified; a warning if replaced on disk                                                                 |
| Security & provenance | Fedora package / Windows signature and publisher, Windows version resource, SHA-256 on demand                                       |
| Scheduling            | real priority (nice value or Windows class), CPU affinity                                                                           |
| Process control       | Suspend, Resume, End process, End process tree, Force kill (Linux) — each with its reason when disabled                             |

Command lines, arguments and environment variables are **not** collected.

### Lazy fields

| Field               | Request                                     |
| ------------------- | ------------------------------------------- |
| details             | `get_process_details` when the drawer opens |
| package / signature | `get_process_provenance`, right after       |
| SHA-256             | `compute_process_sha256`, only on click     |

After an action the drawer re-reads its details **once**. There is no timer.

### Partial availability

Every field is a value **or** a reason (`permissionDenied`, `notDetected`,
`unsupported`, …). A process owned by another user keeps its PID, name, state
and start time and says which fields were refused. A kernel thread has no
executable, no architecture, no package and no action — and says so instead of
showing empty cells.

## Start time vs identity

The _Started_ line is a wall-clock date:

- **Linux** — `btime` from `/proc/stat` + `starttime` (field 22 of
  `/proc/<pid>/stat`) ÷ clock ticks. `btime` has one-second resolution.
- **Windows** — the creation `FILETIME` from `GetProcessTimes`, converted from
  1601 to Unix time.

The **identity token** is the raw tick count / FILETIME, kept inside the
instance ID (`process:<pid>-<token>`) and only ever compared, never displayed as
a date.

## Owner and category

- **Linux** — the _real_ UID from `/proc/<pid>/status`, resolved with
  `getpwuid_r`. Category: `PF_KTHREAD` → _Kernel thread_; UID 0 → _System_;
  PULSE's own UID → _User_; anything else → _Unknown_.
- **Windows** — `OpenProcessToken` + `GetTokenInformation(TokenUser)`, shown as
  `DOMAIN\name (SID)` via `LookupAccountSidW` / `ConvertSidToStringSidW`.
  `S-1-5-18/19/20` → _System_. A refused token is `permissionDenied`, never a
  guessed "SYSTEM". On a domain-joined machine, Windows itself may consult the
  domain controller to resolve a domain account name; PULSE issues no request.

Categories are never inferred from a name. The _All / User / System / Kernel_
filter above the tables hides nothing permanently.

## Architecture

- **Linux** — the first 20 bytes of `/proc/<pid>/exe`: ELF class and
  `e_machine` (`x86_64`, `x86`, `aarch64`, …). Nothing else of the file is parsed.
- **Windows** — `IsWow64Process2`, resolved at run time; fallback
  `IsWow64Process` + `GetNativeSystemInfo`. Limitation: an x64 process emulated
  on ARM64 is not WOW64 and is reported as the native machine by this API.

## Display names

Obscure names (`2.1.278`, `Isolated Web Content`, `Utility Process`) are
explained with real metadata only: executable path and file name, Windows
`ProductName`/`FileDescription`/`CompanyName`, publisher, Fedora package,
parent. When the product name differs from the process name, both are shown.
PULSE never invents a name that no source states.

## Context menu

Right-click a process row, press Shift+F10 or the Menu key on a focused row:

```text
Inspect details
Search online
Open file location
Copy ›            Process name · PID · Executable path · Process instance ID · SHA-256
Suspend
Resume
End process
End process tree
Force kill        (Linux only)
Set priority ›    presets, with the current one checked
Set affinity…
```

↑/↓ move, → or Enter opens a submenu, ← closes it, Escape closes the menu and
returns focus to the row, an outside click closes it, and the menu closes by
itself if a Refresh shows its process is gone. Destructive items stay disabled
("Checking permissions…") until the process's capabilities have been read, and
every disabled item shows its reason inline.

Application rows offer only _Inspect application_, _Search online_ and _Show
processes_: an application groups processes that may have different owners and
permissions, so "end this application" would be ambiguous.

See also [controls.md](controls.md) and [provenance.md](provenance.md).
