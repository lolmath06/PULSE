# Processes and applications

> **Phase 8.** What is running on this machine, what it is using, and — the
> decision this whole document exists to justify — why almost none of it is in
> the metric catalog.

---

## 1. The architectural decision: two tiers, not one

Every other family in PULSE goes through the metrics engine. A GPU publishes
eleven metrics, a disk thirteen, a network interface eleven, and a dashboard
saves a `MetricRef` that still resolves months later.

Processes deliberately do not work that way.

```text
┌── MetricsEngine ─────────────────────────────────────────┐
│  linux.processes / windows.processes                     │
│    process.count.total                                   │
│    process.count.running                                 │
│    process.thread.count.total                            │
│                                                          │
│  3 metrics. On every machine. Forever.                   │
└──────────────────────────────────────────────────────────┘

┌── ProcessSnapshotService ────────────────────────────────┐
│  several hundred rows, renewed on every refresh,         │
│  requested through get_process_snapshot and discarded    │
└──────────────────────────────────────────────────────────┘
```

### Why the catalog is the wrong container for a process

A desktop runs three to five hundred processes. Most of them live for less than
a second. Publishing the six per-process figures through the engine would mean:

```text
~350 processes × 6 metrics   ≈  2 100 MetricDefinitions
replaced wholesale            on  every single refresh
```

That is not merely expensive. It breaks what a `MetricDefinition` **is**.

A catalog entry is a _promise_: a widget saves `cpu.usage.total@cpu:system`
today and expects it to resolve in six months, on a different boot, possibly on
the other operating system. `process:1234-9001` stops resolving the moment that
process exits — which, for most processes, is before the user has finished
reading the row. A catalog full of such references is a catalog of promises
that cannot be kept.

There are three further consequences, each bad on its own:

| Consequence               | Why it matters                                                                |
| ------------------------- | ----------------------------------------------------------------------------- |
| Catalog churn             | `get_metric_catalog` is discovered once on mount by every card; it would move |
| `sample_metrics` misuse   | A call shaped for tens of references would carry thousands                    |
| Meaningless engine counts | `metricCount` would say "2 114" and mean nothing about the machine            |

So PULSE publishes the three figures that genuinely describe the machine
through the engine, and everything per-process through a service and a command
of its own. The separation is asserted by tests, not merely documented:
`a_snapshot_never_registers_anything_in_the_metric_catalog` takes two snapshots
and proves the catalog did not grow, and
`neither_platform_declares_a_metric_for_an_individual_process` proves neither
provider declares a source other than `process:system`.

### What each tier costs

| Tier                | Call                   | Cost on the development machine         |
| ------------------- | ---------------------- | --------------------------------------- |
| Machine-wide counts | `sample_metrics`       | one `/proc/<pid>/stat` read per process |
| Full snapshot       | `get_process_snapshot` | ~20 ms for 700 processes (measured)     |

The provider does **not** reuse the full walk: it needs three numbers, and
`/proc/<pid>/stat` alone carries all three, so it skips the I/O counters, the
executable links and the ownership checks entirely.

---

## 2. Machine-wide metrics

| Metric                       | Unit  | Kind  | Linux source                        | Windows source                     |
| ---------------------------- | ----- | ----- | ----------------------------------- | ---------------------------------- |
| `process.count.total`        | count | gauge | entries in `/proc`                  | `CreateToolhelp32Snapshot` entries |
| `process.count.running`      | count | gauge | field 3 of `/proc/<pid>/stat` = `R` | **unsupported** — see below        |
| `process.thread.count.total` | count | gauge | field 20 of `/proc/<pid>/stat`      | `PROCESSENTRY32W.cntThreads`       |

All three are published on `process:system`, the single `process:` source PULSE
ever registers.

**`process.count.running` is unavailable on Windows, and that is deliberate.**
Windows schedules _threads_; a process is a container with no state of its own.
PULSE could ask `NtQuerySystemInformation` for every thread's state and call a
process "running" when any of its threads is — but that figure would be PULSE's
invention rather than the operating system's answer, and the same column would
then mean two different things on the two platforms PULSE treats as equals. It
is declared with an `unsupported` availability and a reason, so the interface
can explain the gap rather than silently omit a metric Fedora has.

`process.thread.count.total` counts only threads PULSE could actually read. On
a machine full of protected processes it is an **understatement**, never an
invention — which is the honest direction to be wrong in.

---

## 3. Process identity: a PID is not an identity

Operating systems recycle process identifiers. Fedora wraps at
`/proc/sys/kernel/pid_max`; Windows reuses them aggressively and with no
ordering guarantee. So this sequence is entirely ordinary:

```text
10:00:00   PID 1234 = firefox     baseline recorded: 812 s of CPU time
10:00:03   firefox exits
10:00:04   PID 1234 = cargo       counter now reads 0.2 s
```

A tracker keyed on the PID alone differences `0.2 s` against `812 s`, gets a
negative delta, and either clamps it to zero (hiding real work) or wraps it
into an astronomical positive — a 10 000 % CPU spike. Both are exactly the kind
of confidently-wrong number PULSE exists not to produce.

### The fix

Identity is `(pid, start_token)`:

| Platform | Start token                          | Meaning                     |
| -------- | ------------------------------------ | --------------------------- |
| Linux    | field 22 of `/proc/<pid>/stat`       | clock ticks after boot      |
| Windows  | `GetProcessTimes` → `ftCreationTime` | 100 ns intervals since 1601 |

PULSE never _interprets_ either — it never turns the Windows value into a date.
It only needs something stable for one incarnation and different for the next.
Rendered as `process:1234-9001`, which obeys the `SourceId` grammar (a test
proves it) but is **never registered in the catalog**.

Every rate baseline is keyed on this pair, so a recycled PID starts from no
baseline instead of inheriting its predecessor's. Two tests pin it:
`a_reused_pid_never_inherits_its_predecessors_baseline` in `rates.rs` and
`a_reused_pid_is_a_new_row_with_no_inherited_rates` in `service.rs`.

### The one place the token is weaker

On Windows, a process whose handle `OpenProcess` refuses keeps a start token of
zero, because `GetProcessTimes` is what supplies it. For as long as that PID is
that process the identity is stable, which is what the baselines need; if a
protected PID were recycled into another protected process within one refresh
interval, the two would share an identity. Protected processes are also the
ones with no CPU or I/O counters to inherit, so the consequence is nil — but it
is a real narrowing and it is stated rather than hidden.

---

## 4. CPU: one convention, both platforms

```text
                delta CPU time
percent =  ─────────────────────────────  × 100
           delta wall time × logical CPUs
```

**0–100 % is a share of the machine's entire CPU capacity.**

On the 32-thread development machine, one thread saturating one logical
processor is **3.125 %**, not 100 %.

This is the Windows Task Manager convention rather than the `top` convention,
and it is chosen for one reason:

```text
sum(process CPU)  ≈  cpu.usage.total
```

That identity is what makes the process list _explain_ the system gauge instead
of contradicting it. The alternative — `3200 %` on Fedora and `100 %` on
Windows for the same workload, as most monitors do — makes the column
meaningless across the two platforms PULSE treats as equals.

Measured on Fedora: a `yes` process spawned by the audit harness reported
**3.1215 %**, against a theoretical 3.125 % for one of 32 logical processors.

### Units

Linux reports clock ticks (`USER_HZ`, from `sysconf(_SC_CLK_TCK)`); Windows
reports 100 ns `FILETIME` intervals. Both collectors convert to **nanoseconds**
before handing the number over, so the delta arithmetic is one shared,
platform-free function.

`cutime`/`cstime` — the CPU time of _reaped children_ — are deliberately
excluded on Linux. Including them would attribute a finished build's CPU time
to the shell that launched it, and count the same work twice across the table.

### What is guarded against

| Situation                                | Answer                             |
| ---------------------------------------- | ---------------------------------- |
| first snapshot, or a newly seen process  | waiting for another sample         |
| zero elapsed time                        | waiting; baseline kept             |
| counter moved backwards (reset, suspend) | waiting; baseline replaced         |
| process vanished                         | no row; baseline pruned            |
| PID reused                               | different identity, so no baseline |
| absurd delta                             | clamped to the machine's capacity  |
| unknown logical processor count          | no CPU value; I/O still measured   |

Nothing in the rate path can produce `NaN`, `Infinity` or a negative number:
every result passes through one `finite_non_negative` check.

---

## 5. Memory: RSS and Working Set

`residentMemoryBytes` means **the physical memory this process currently
occupies**.

| Platform | Source                                       | Mechanism         |
| -------- | -------------------------------------------- | ----------------- |
| Linux    | field 24 of `/proc/<pid>/stat` × page size   | resident set size |
| Windows  | `K32GetProcessMemoryInfo` → `WorkingSetSize` | working set       |

These are **not the same kernel mechanism**. A Linux RSS counts pages mapped
and resident; a Windows working set counts pages the memory manager is keeping
available to the process, and its trimming policy is different. They are,
however, the same _user-facing notion_ — "how much real memory is this program
using right now" — and both `top` and Task Manager put them in the same column
for the same reason.

Neither counts shared pages once. Several processes sharing a library each
report its resident pages, so **an application's memory sum over-counts what
shared libraries occupy**. That is stated in the limitations rather than
silently corrected, because correcting it properly needs per-page accounting
neither platform offers cheaply.

`memoryPercent` is `residentMemoryBytes / physicalMemoryTotal × 100`, computed
with checked arithmetic: a zero or unknown denominator yields no measurement
rather than `NaN` or `Infinity`, and the result is clamped to 100 %.

---

## 6. Process I/O: the same columns, two different counters

| Platform | Source                 | Fields                                    | What they count                                                                          |
| -------- | ---------------------- | ----------------------------------------- | ---------------------------------------------------------------------------------------- |
| Linux    | `/proc/<pid>/io`       | `read_bytes`, `write_bytes`               | bytes the kernel's block layer attributes to the process (storage-backed)                |
| Windows  | `GetProcessIoCounters` | `ReadTransferCount`, `WriteTransferCount` | bytes transferred by the process's read / write I/O operations, as Windows accounts them |

> **Corrected in Phase 9.** Phase 8 described both platforms as "storage
> traffic". That is true of Linux `read_bytes`/`write_bytes`, and **not proven**
> for Windows: Microsoft documents the transfer counts as the bytes of all read
> and write operations the process performs, which is not restricted to a block
> device (file reads served from cache and I/O through other device drivers
> can count). The UI keeps the column names _Read_ and _Write_; the numbers are
> unchanged; only the claim that they mean the same thing on both platforms is
> withdrawn. Compare Read/Write across platforms with that in mind.

### Why not `rchar`/`wchar`

`/proc/<pid>/io` offers two pairs and they measure different things:

| Field                        | Counts                                        |
| ---------------------------- | --------------------------------------------- |
| `rchar` / `wchar`            | every byte passed to `read(2)` / `write(2)`   |
| `read_bytes` / `write_bytes` | bytes that reached or left a **block device** |

`rchar` includes bytes served from page cache, read from a pipe, a socket, a
tty or `/proc` itself. Using it, a process re-reading a cached file would be
reported at gigabytes per second off a disk whose activity light never blinked
— and PULSE's own `storage.io.*` metrics, which come from the block layer,
would disagree with PULSE's own process table on the same screen.

The rest of this section is about Linux. The cost of the honest choice: **a process doing purely cached, tmpfs or
network I/O reports `0 B/s`.** That is the correct answer to "what is this
process doing to my storage". Verified on Fedora: the same 64 MiB write reports
`0 B/s` on tmpfs and `1750.3 MiB/s` on btrfs.

A second, filesystem-specific effect is worth knowing: on btrfs and other
filesystems with delayed allocation, writeback is performed by kernel worker
threads, so a process's own `write_bytes` may stay at zero while
`btrfs-transaction` does the work. PULSE reports what the kernel attributes.

Rates are `delta / elapsed`. The first snapshot reports `waiting`; after a real
baseline, `0 B/s` is a genuine measurement and is shown as such.

---

## 7. Process state

Five states, mapped only where the semantics are honest:

| PULSE               | Linux           | Windows           |
| ------------------- | --------------- | ----------------- |
| `running`           | `R`             | —                 |
| `sleepingOrWaiting` | `S` `D` `I`     | —                 |
| `stopped`           | `T` `t`         | —                 |
| `zombie`            | `Z`             | —                 |
| `other`             | everything else | **every process** |

Interruptible and uninterruptible sleep are one state on purpose: the
difference matters to a kernel engineer and to nobody looking at a process
list. An unrecognised letter degrades to `other` rather than failing, so a
future kernel state cannot break the table.

On Windows every process is `other`, carrying the reason as an availability —
see §2.

---

## 8. Classification

Three values: `userApplication`, `systemProcess`, `unknown`. Classified only
from signals that are actually reliable.

**Linux**

1. `PF_KTHREAD` (bit `0x00200000` of field 9) — the kernel's own marker for a
   kernel thread. Not a guess from a bracketed name or a zero resident set: a
   user program is entitled to be named `[something]` and to be swapped out.
2. The owner of `/proc/<pid>`, from one `stat` call: `uid 0` is a system
   process, the user PULSE runs as is a user application.
3. Anything else — a third user's process, an unreadable directory — is
   `unknown`.

**Windows**

1. PID 0 and PID 4 — the Idle and System processes, fixed by the OS.
2. The executable lives under `%SystemRoot%` — boundary-aware, so
   `C:\WindowsApps\…` is correctly _not_ inside `C:\Windows`.
3. No readable path, or no known system root — `unknown`.

Notably absent: guessing from the name. A list of `svchost`, `csrss` and
`explorer` would be wrong on the next Windows release and trivially spoofable
by any program that renames itself. PULSE classifies from where a file _is_.

---

## 9. Application aggregation

Modern programs are process swarms: Firefox runs a parent plus a content
process per few tabs, a GPU process, a socket process and several utilities.
Thirteen `firefox` rows say what is running; one Firefox row with thirteen
processes says what is using the machine.

### Grouping key

| Priority | Key                          | Confidence   |
| -------- | ---------------------------- | ------------ |
| 1        | the resolved executable path | `executable` |
| 2        | the process name             | `name`       |

Grouping by name alone is wrong in the other direction: `python`, `node`,
`java`, `sh` and `electron` name _runtimes_, not applications, and folding two
unrelated programs together because they share an interpreter produces a row
whose CPU total means nothing. So `/usr/bin/python3` and
`/opt/tool/venv/bin/python3` stay two applications.

Windows paths are compared case-insensitively (NTFS is); POSIX paths are not,
because there they genuinely name different files.

The key is internal and deterministic. It is **not** a `SourceId` and is stored
in nothing that outlives the window: it contains a filesystem path, which may
include a user's home directory and therefore their name.

### Display name

The executable's own file name, minus its path and a trailing `.exe`. No icon
database, no `.desktop` lookup, no capitalisation invented for a program PULSE
knows nothing about. One narrow exception: a file name that is digits and dots
only — some programs install each release as
`~/.local/share/thing/versions/2.1.278` — falls back to the process name,
because an application called `2.1.278` names nothing.

### Sums

Process count, thread count, CPU, resident memory, read and write are added.

**A sum is of what was measured, never of what was assumed.** If three of an
application's five processes report CPU and two are refused, the total is the
three — an understatement, but a true one. If _nothing_ was measured, the
reason is carried up (waiting first, then a refusal), so an application whose
processes are all still waiting for a baseline says "waiting" rather than
"idle".

### Limitations

- Resident memory over-counts shared pages (§5).
- Name-based grouping can merge two unrelated programs; the interface marks
  those rows and says so in a tooltip.
- A process with a readable path and one without do not merge, even when they
  are obviously the same program: PULSE cannot prove it.
- Applications are not identified across reboots. There is no persistent
  application identity in this phase, by design.

---

## 10. Partial availability

Availability is per **field**, never per row.

```text
process        visible
  cpu          available
  memory       available
  read/write   permissionDenied
  exe path     permissionDenied
```

A process whose base inventory exists stays in the table. Dropping the row
because two of its six fields are refused would hide something that is
genuinely running; printing `0 B/s` for the refused counters would be a lie.

| Cause                                    | Availability             |
| ---------------------------------------- | ------------------------ |
| another user's `/proc/<pid>/io` or `exe` | `permissionDenied`       |
| Windows `OpenProcess` refused            | `permissionDenied`       |
| kernel thread has no executable or I/O   | `notDetected`            |
| kernel built without I/O accounting      | `notDetected`            |
| no baseline yet                          | `temporarilyUnavailable` |
| process exited mid-read                  | `temporarilyUnavailable` |
| Windows process-level state              | `unsupported`            |

Errors are never collapsed into `unsupported`. On the development machine, 125
of 702 processes report `permissionDenied` on I/O and executable path, and all
125 keep every other column.

---

## 11. Permissions

**PULSE does not run as root or as administrator, and does not ask to.**

On Fedora the consequence is precise: `/proc/<pid>/io` and `/proc/<pid>/exe`
are owned by the process's own user, so another user's processes lose those two
fields and keep the rest.

On Windows, `OpenProcess` legitimately fails for protected processes — parts of
anti-malware, `csrss`, the System process — even for an administrator. PULSE
asks for `PROCESS_QUERY_LIMITED_INFORMATION`, the narrowest right that answers
these questions, and where it is refused keeps the Toolhelp inventory row: PID,
parent PID, thread count and image name are available without any handle at
all.

---

## 12. Privacy

PULSE collects **no command line, no arguments and no environment variables**.

They routinely carry file paths, URLs, database connection strings, API tokens
and, occasionally, passwords. Phase 8 has no use for any of it: the process
name identifies the program and the executable path groups it. A test asserts
the wire format carries no `commandLine`, `arguments`, `environment`, `cmdline`
or `argv` field.

The executable path _is_ collected, because it is the only trustworthy grouping
key — but it is shown only as a tooltip, never as a column, and never enters a
`SourceId` or any saved configuration.

---

## 13. Observation, inspection, control

The **snapshot** path (this document) observes only: `/proc`,
`CreateToolhelp32Snapshot`, `GetProcessTimes`, `K32GetProcessMemoryInfo`,
`GetProcessIoCounters`, `QueryFullProcessImageNameW`. It never hashes, never
queries a package or signature, and never changes a process.

Since Phase 9, two separate services sit beside it:

- `ProcessInspectorService` — reads **one** selected process in depth, lazily.
  See [../processes/inspector.md](../processes/inspector.md) and
  [../processes/provenance.md](../processes/provenance.md).
- `ProcessControlService` — suspends, resumes, ends, reprioritises and re-pins
  **one** process instance, only on an explicit, confirmed click, after
  re-validating PID and start token. See
  [../processes/controls.md](../processes/controls.md).

Neither is a metric provider; the catalog does not grow. No code path reads
process memory, debugs, or injects into a process.

---

## 14. Handles and lifetimes

On Windows, every handle is wrapped in a guard that closes it on drop,
including on early returns. The cycle is strictly:

```text
enumerate → open → read → close
```

**No handle is kept between refreshes.** The rate baselines hold an identity
and two integers. Keeping four hundred handles open across refreshes would pin
every one of those processes' kernel objects in memory for as long as PULSE
ran.

On Linux there is nothing to keep: every read opens and closes a `/proc` file
within one call.

---

## 15. Collection strategy and cost

### Fedora

Per process, at most:

```text
1  open + read   /proc/<pid>/stat     name, state, parent, threads, CPU, RSS, start token
1  open + read   /proc/<pid>/io       skipped for kernel threads
1  readlink      /proc/<pid>/exe      skipped for kernel threads
1  stat          /proc/<pid>          the owning user
```

plus one `read_dir` of `/proc`, one `/proc/meminfo` and one `/proc/stat` for
the whole pass. Roughly **1 900 syscalls for 700 processes**.

`/proc/<pid>/statm` and `/proc/<pid>/status` are deliberately not read: `stat`
already carries the resident set and the thread count.

### Windows

One `CreateToolhelp32Snapshot` for the inventory, then per process one
`OpenProcess`, four reads and one `CloseHandle`.

### What is refused

- one command per process (`ps`, `top`, `tasklist`, `wmic`, PowerShell)
- one Tauri call per process
- one `MetricsEngine` sample per process
- one thread per process

The walk is sequential. Measured at **16–26 ms for 700 processes** on the
development machine, which is well inside interactive. Bounded parallelism
would be justified only by a measurement showing it is not, and the snapshot
reports its own duration so that measurement is always available.

### The `/proc/<pid>/stat` parser

`/proc/<pid>/stat` is one space-separated line, which invites exactly one
implementation and punishes it. Field 2 is `comm`, wrapped in parentheses and
**not escaped**. Real names on a running Fedora desktop include `Web Content`
(a space), `foo (bar)` (nested parentheses) and `kworker/3:1H-events`.

A `split_whitespace()` over the whole line shifts every later field by the
number of spaces in the name. The consequence is not a parse error — it is a
_successful_ parse of the wrong columns: `starttime` becomes garbage, so the
identity changes every refresh, so no baseline ever matches, so the CPU column
is permanently blank for exactly the processes the user most wants to see.

PULSE splits on the **last** `)` in the line and has a test asserting that the
naive approach genuinely disagrees.

---

## 16. Manual refresh only

No polling, no `setInterval`, no background sampler. One snapshot on mount, one
per click of _Refresh_. A frontend test advances timers by two minutes and
asserts no further request is made.

The service also takes **no baseline at startup**: priming would make the first
snapshot show rates derived from an interval the user never asked for. The
first snapshot establishes the baseline and says `waiting for another sample`;
the second — the user's first _Refresh_ — carries real rates. Memory and thread
counts are snapshot values and appear immediately.

After a baseline exists, `0 %` and `0 B/s` are **real measurements** and are
shown as such, never as a dash.

---

## 17. Frontend API

| Command                | Returns                                                  |
| ---------------------- | -------------------------------------------------------- |
| `get_process_snapshot` | `ProcessSnapshot` — counts, rows, applications, duration |

Called through `src/services/processes.ts`. Types in `src/types/processes.ts`.
Deliberately **not** routed through `sample_metrics`, for the reasons in §1.

---

## 18. What is deliberately absent

- no history and no trend per process
- no process tree view
- no icons, no `.desktop` parsing, no Windows resource extraction
- no per-process GPU or network attribution
- no open files, sockets or handles
- no command line, arguments or environment (§12)
- no kill, suspend, resume or priority change (§13)
- no persistent application identity across reboots
- no per-process history in the metrics engine (§1)

## 19. Cross-references

- [`README.md`](README.md) — engine principles and the catalog shape
- [`identifiers.md`](identifiers.md) — `SourceId` grammar and the `process` kind
- [`providers.md`](providers.md) — the provider contract
- [`../platforms/fedora.md`](../platforms/fedora.md) — `/proc` details
- [`../platforms/windows.md`](../platforms/windows.md) — the Windows API surface
