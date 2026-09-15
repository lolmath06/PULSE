# CPU and Memory Metrics

> Phase 2. PULSE's first real system metrics, implemented natively on both
> Fedora Linux and Windows.

These five metrics are the architectural reference for every metric that
follows. They are deliberately few and deliberately exact.

## The contract

| Metric                 | Source          | Unit      | Kind  | Fedora backend  | Windows backend        |
| ---------------------- | --------------- | --------- | ----- | --------------- | ---------------------- |
| `cpu.usage.total`      | `cpu:system`    | `percent` | gauge | `/proc/stat`    | `GetSystemTimes`       |
| `memory.total`         | `memory:system` | `bytes`   | gauge | `/proc/meminfo` | `GlobalMemoryStatusEx` |
| `memory.used`          | `memory:system` | `bytes`   | gauge | `/proc/meminfo` | `GlobalMemoryStatusEx` |
| `memory.available`     | `memory:system` | `bytes`   | gauge | `/proc/meminfo` | `GlobalMemoryStatusEx` |
| `memory.usage.percent` | `memory:system` | `percent` | gauge | derived         | derived                |

Providers: `linux.cpu` and `linux.memory` on Fedora; `windows.cpu` and
`windows.memory` on Windows.

**Only `providerId` differs between the platforms.** Key, source, unit, kind,
value type, display name, description and source label are identical, and a
contract test that runs on both platforms asserts exactly that. A widget bound
to `memory.used@memory:system` moves from Fedora to Windows unchanged.

## The PULSE memory convention

Both platforms report two raw numbers — total and available physical memory, in
bytes — and PULSE derives the rest **identically**:

```text
used          = total - available
usage_percent = used / total * 100
```

So `used + available == total` always holds exactly, on both operating systems.

### Why `available` and not `free`

On Linux, `MemFree` excludes reclaimable page cache. A healthy machine that has
cached a few gigabytes of files would look nearly out of memory — the single
most common mistake in memory reporting. `MemAvailable` is the kernel's own
estimate of what a new allocation could actually obtain, and has existed since
Linux 3.14.

Windows' `ullAvailPhys` carries the same meaning, which is why the two
platforms can share one definition rather than two that quietly disagree.

### Why PULSE computes the percentage itself

Windows offers `dwMemoryLoad`, a ready-made integer percentage. PULSE ignores
it and computes from the byte counts instead, for two reasons: it is rounded to
a whole number, so it would disagree with the `used` and `total` figures shown
beside it; and it would differ from how Fedora reports the same thing.

## CPU usage is a rate, not a reading

Both operating systems expose **monotonic counters** of time spent busy and
idle. A single absolute read says nothing about current load — usage only
exists _between two samples_:

```text
usage = (busy_now - busy_before) / (total_now - total_before) * 100
```

The platform-specific part is small: extract `busy` and `total` from the native
counters. Everything after that — the delta, the division, the clamping to
0–100, and the baseline state machine — is shared code in
`metrics/wellknown/cpu.rs`, written once and tested once for both platforms.

### Fedora: `/proc/stat`

The aggregate `cpu` line gives ten jiffy counters:

```text
cpu  user nice system idle iowait irq softirq steal guest guest_nice
```

```text
idle_all = idle + iowait
total    = user + nice + system + idle + iowait + irq + softirq + steal
busy     = total - idle_all
```

Two details that are easy to get wrong:

- **`iowait` counts as idle.** The CPU is genuinely executing nothing while a
  task waits for I/O.
- **`guest` and `guest_nice` must not be added to the total.** The kernel
  already includes guest time inside `user`, and guest_nice inside `nice`.
  Adding them again inflates the total and makes a busy host look idle. This is
  the classic `/proc/stat` bug, and there is a test named after it.

Kernels vary: only the first four fields are guaranteed. PULSE requires those
four, treats later fields as zero when absent, and ignores fields a future
kernel might append.

### Windows: `GetSystemTimes`

Returns three `FILETIME` values — idle, kernel and user time, in 100-nanosecond
units.

```text
total = kernel + user      (kernel ALREADY includes idle)
busy  = total - idle
```

**Windows counts idle time inside `kernel`.** Applying the Linux formula here
would report a completely idle machine as heavily busy. This is the single
Windows-specific trap in this phase, and it has its own test.

A `FILETIME` is a 64-bit value split across two 32-bit fields; Microsoft's
documentation is explicit that it must not be cast directly to a `u64`. PULSE
recombines the halves in `filetime_to_u64`.

## The baseline, and why the first sample may say "waiting"

PULSE has no sampler thread yet. The naive way to produce a delta inside a
command would be to read, `sleep(100ms)`, read again — which would block the UI
on every request.

Instead each CPU provider **captures a baseline when it is constructed**, at
application startup, and every request compares against the previous reading.

If no usable delta exists, the sample is
`temporarilyUnavailable` with a reason, **never `0%`**. Reporting an unmeasured
CPU as idle is a lie the user cannot distinguish from a genuinely idle machine.
Three cases produce it, and all three store a fresh baseline so the _next_
request succeeds:

| Case                    | When                                           |
| ----------------------- | ---------------------------------------------- |
| `NoBaseline`            | First reading; priming at startup failed       |
| `NoElapsedTime`         | Counters did not advance between the two reads |
| `CountersWentBackwards` | Suspend/resume, or a virtualised clock reset   |

Memory has no such problem: it is an instantaneous reading, available from the
very first request.

## Robustness

Everything that could produce a nonsensical number is refused before it reaches
a widget:

| Guard                               | Behaviour                                                    |
| ----------------------------------- | ------------------------------------------------------------ |
| `total == 0`                        | Rejected — nothing to divide by                              |
| `available > total`                 | Rejected — `used` would underflow to a colossal number       |
| `busy > total`                      | Rejected — usage would exceed 100%                           |
| Counter overflow                    | Rejected via checked arithmetic                              |
| Non-finite result                   | Refused at `MetricValue::number`                             |
| Unexpected `/proc/meminfo` unit     | Rejected rather than guessed (a wrong guess is off by 1024×) |
| Missing `MemTotal` / `MemAvailable` | Reported, never silently substituted                         |
| Poisoned baseline mutex             | Structured internal error, never `unwrap()`                  |

A failed read becomes the availability state that describes it: a busy `/proc`
is `temporarilyUnavailable`, an `EACCES` is `permissionDenied`, a malformed
file is `providerError`. Never a zero, and never a blanket `unsupported`.

## Privileges

**None required.** `/proc/stat` and `/proc/meminfo` are world-readable;
`GetSystemTimes` and `GlobalMemoryStatusEx` are available to any Windows
process. If a future change to these five metrics starts needing elevation,
something has gone wrong.

## Cost

One `/proc` read per family per request on Fedora, one syscall per family on
Windows. No subprocess, no shell, no `wmic`, no PowerShell, no WMI, no `/sys`
traversal, no `sensors`. A CPU + memory request is a handful of reads.

Because the four memory metrics come from a **single** reading, they are always
mutually consistent — there is no torn read where `used` and `available` come
from different moments and fail to add up.

## Dependencies

Fedora needs none beyond the standard library. Windows uses `windows-sys` —
raw FFI bindings with no wrapper layer and no runtime — with exactly the three
features that declare the two calls PULSE makes, under
`[target.'cfg(target_os = "windows")'.dependencies]`.

No cross-platform monitoring crate was added. Validating that PULSE's own
platform architecture works natively is a large part of what this phase is for.

## Testing

Parsing and arithmetic are **pure functions taking their input as arguments**,
so they are tested with fixtures rather than against a live kernel:
`parse_proc_stat`, `parse_meminfo`, `counters_from_system_times`,
`filetime_to_u64`, `usage_percent`, `MemoryReading`.

Both platform modules compile on **every** host — only the FFI and the
`HostPlatform` implementations are gated — so a Fedora test run exercises the
Windows arithmetic, and a Windows run exercises the `/proc` parsers. There is
no reason for Windows maths to be untestable from a Linux machine.

Host-specific tests assert **invariants only**, never a particular amount of
RAM or a particular load:

```text
memory.total > 0
memory.available <= memory.total
memory.used + memory.available == memory.total
0 <= memory.usage.percent <= 100
0 <= cpu.usage.total <= 100      (when the sample is available)
```

A first CPU sample reporting `temporarilyUnavailable` is a valid outcome and
the tests accept it.

## Deliberately not in this phase

Per-core CPU, CPU frequency, load average, temperatures, fans, swap, GPU,
storage, network, processes and battery. Temperature in particular is a phase
of its own: its availability depends heavily on hardware and it is genuinely
hard on Windows (see [`../platforms/windows.md`](../platforms/windows.md)).

There is also still **no scheduler**: the model remains
`request → sample → response`, with the UI refreshing on demand.
