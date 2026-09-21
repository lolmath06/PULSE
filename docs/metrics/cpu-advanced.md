# Advanced CPU Metrics

> Phase 3. Per-logical-processor usage and frequency, plus CPU topology, on
> both Fedora Linux and Windows behind one contract.

Phase 2 published a single CPU number for the whole machine. Phase 3 answers
the question a user actually asks:

> How many physical cores and logical processors does my CPU have, what is each
> logical processor doing right now, and at what frequency does the OS report it
> running?

## Logical processor, physical core, package

Three words everyday speech conflates and PULSE never may:

| Term                  | Meaning                                            | Metric               |
| --------------------- | -------------------------------------------------- | -------------------- |
| **Processor package** | One physical chip in one socket                    | `cpu.count.package`  |
| **Physical core**     | One execution core inside a package                | `cpu.count.physical` |
| **Logical processor** | One hardware thread the scheduler can run tasks on | `cpu.count.logical`  |

A user saying _"CPU usage per core"_ almost always means **per logical
processor**. With simultaneous multithreading (Intel Hyper-Threading, AMD SMT)
one physical core exposes two logical processors, and `/proc/stat`, Windows and
every scheduler report usage per logical processor — never per core. There is
no honest way to split a core's usage between its two threads, so PULSE does
not pretend to: the metric is called `cpu.usage.logical` and its description
says what it measures.

On a hybrid CPU the distinction sharpens. On the Intel Core i9-14900HX this was
developed against:

```text
8 P-cores × 2 threads = 16 logical processors
16 E-cores × 1 thread = 16 logical processors
                        ──────────────────────
24 physical cores       32 logical processors
```

`logical / physical` is **not** a constant, and nothing in PULSE may assume it
is — not two, not one.

## The metrics

Machine-wide, on `cpu:system`:

| Metric               | Unit      | Kind  | Value type |
| -------------------- | --------- | ----- | ---------- |
| `cpu.usage.total`    | `percent` | gauge | number     |
| `cpu.count.logical`  | `count`   | state | number     |
| `cpu.count.physical` | `count`   | state | number     |
| `cpu.count.package`  | `count`   | state | number     |

Per logical processor, on `cpu:logical-N`:

| Metric                  | Unit      | Kind  | Value type |
| ----------------------- | --------- | ----- | ---------- |
| `cpu.usage.logical`     | `percent` | gauge | number     |
| `cpu.frequency.current` | `hertz`   | gauge | number     |
| `cpu.frequency.max`     | `hertz`   | gauge | number     |

Per processor package, on `cpu:package-N` (Phase 5):

| Metric                    | Unit      | Kind  | Value type |
| ------------------------- | --------- | ----- | ---------- |
| `cpu.temperature.package` | `celsius` | gauge | number     |

`cpu.temperature.package` is the package's own sensor — never an average of the
per-core channels beside it, and never one of the thermal limits. It is
implemented on Fedora through `hwmon` and `unsupported` on Windows, which has no
interface for it that does not require a kernel-mode driver. See
[`thermals.md`](thermals.md).

### Why the counts are `state` and not `gauge`

A core count does not rise and fall, and averaging one over time is meaningless:
_"this machine had 23.6 physical cores last hour"_ is not a sentence PULSE
should ever be able to produce.

`MetricKind::State` is documented in the Phase 1 model as "a discrete
condition", which is what a core count is — a fact about the machine, not a
reading taken from it. Crucially, `MetricKind::is_directly_averageable()`
returns `false` for it, so the aggregation rules arriving with history in a
later phase will refuse to average these **by construction**, rather than
relying on a widget author to know better.

The unit is `count` and the value type is `number`, so a widget still renders
`24` without special-casing.

## The catalog is sized by the machine

The CPU metric set is **not a fixed list**. For `N` logical processors:

```text
 4   memory.*
 1   cpu.usage.total
 3   cpu.count.*
 N   cpu.usage.logical
 N   cpu.frequency.current
 N   cpu.frequency.max
────
 8 + 3N
```

On the development machine that is `8 + 3×32 = 104`. On a four-thread virtual
machine it is `20`. **Nothing anywhere in PULSE hardcodes `N`** — not the
backend, not the contract tests, not the interface. The runtime discovers the
machine, and the same code serves both.

This is why the Metrics engine card shows a number read from the engine rather
than a constant, and why the CPU details card derives its rows from the catalog
rather than from a list of `CPU 0`…`CPU 31`.

## Identity: `cpu:logical-N`

```text
cpu:system         the machine's CPU as a whole
cpu:logical-0      the first logical processor
cpu:logical-31     the thirty-second
```

These are **logical slots of this system, not hardware serial numbers.** The
distinction matters for what a saved dashboard can promise:

- Stable for as long as the machine's CPU configuration is. `cpu:logical-7`
  keeps meaning the same processor across reboots.
- **Not portable between machines.** `cpu:logical-7` on a 32-thread laptop and
  on a 4-thread virtual machine are unrelated; the second has no such slot at
  all. Unlike `storage:nvme0n1` or `gpu:pci-0000-01-00-0`, which are derived
  from device identity, a logical processor ordinal is derived from position.
- Not a claim about the silicon. Ordinal 7 may be an E-core on one machine and
  an SMT sibling on another.

On Linux the ordinal **is** the kernel's own CPU number (`cpu7` →
`cpu:logical-7`), so it lines up with `htop`, `taskset` and everything else a
user might compare against. On Windows it is a PULSE-assigned ordinal; see
below.

### The architectural implication for dashboards

A future dashboard must be able to express:

> every `cpu.usage.logical`

as **one** saved selection, rather than forcing the user to store thirty-two
individual `MetricRef`s. Storing them individually would mean a dashboard built
on this laptop shows twenty-eight broken widgets when opened on a four-thread
machine, and would silently miss processors that appear after a CPU hotplug.

The shape this points at is a _selector_ — "all sources of key `cpu.usage.logical`",
resolved against the catalog at load time — rather than a frozen list of
references.

**Phase 3 does not build this.** It is documented here because the identifier
scheme was chosen to make it possible: every per-processor metric shares one
key and differs only in source, which is exactly the shape a key-wide selector
needs. Widgets and selectors are a later phase.

## Fedora

### Per-logical usage — `/proc/stat`

The same file Phase 2 already read. It contains the aggregate `cpu` line **and**
a `cpuN` line per online logical processor, so **one read feeds
`cpu.usage.total` and all N `cpu.usage.logical` values**. Reading it per metric
would be 33 reads instead of 1 and — worse — would sample processors at
slightly different instants, so the per-processor figures would not reconcile
with the aggregate.

The formula is **identical** at both scopes:

```text
idle_all = idle + iowait
total    = user + nice + system + idle + iowait + irq + softirq + steal
busy     = total - idle_all
usage    = Δbusy / Δtotal × 100
```

`guest` and `guest_nice` are excluded: the kernel already counts guest time
inside `user` and guest_nice inside `nice`. Adding them again inflates the total
and makes a busy host look idle — the classic `/proc/stat` bug. A test asserts
that adding guest time changes nothing, for the aggregate and for a `cpuN` line.

A malformed `cpuN` line is **skipped, not fatal**: `/proc/stat` is generated on
the fly and is not read atomically, so losing one processor's reading is far
better than losing the machine's. A malformed _aggregate_ line is an error,
because nothing else can stand in for it.

### Current frequency — `scaling_cur_freq`

`/sys/devices/system/cpu/cpuN/cpufreq/scaling_cur_freq`, in kHz.

Chosen over `cpuinfo_cur_freq`, which reports the same figure but is
root-readable only on many drivers — PULSE must work unprivileged. With
`intel_pstate` or `amd-pstate` this file asks the hardware for its current
operating point at the moment of the read, which is as close to the truth as an
unprivileged process gets.

### Maximum frequency — `cpuinfo_max_freq`

`/sys/devices/system/cpu/cpuN/cpufreq/cpuinfo_max_freq`, in kHz.

Deliberately **not** `scaling_max_freq`, which is the current _policy_ ceiling:
a laptop in a power-saving profile reports a scaling maximum far below what the
chip can do, and publishing that as "maximum frequency" would be wrong in a way
the user can see. When `cpuinfo_max_freq` is absent, PULSE reports the metric
`unsupported` rather than substituting the policy value.

On the development machine this correctly reports different maxima per core
type — 5.6 GHz and 5.8 GHz for P-cores, 4.1 GHz for E-cores.

### Topology — `/sys/devices/system/cpu/cpuN/topology/`

```text
physical_package_id    which socket
core_id                which core within that package
```

Physical cores are counted as **distinct `(package_id, core_id)` pairs**.
`core_id` is only unique within a package: a dual-socket machine has a `core_id`
0 in package 0 and another in package 1, and counting bare `core_id` values
would report a 2×32-core server as having 32 cores.

### Online processors and gaps

`/sys/devices/system/cpu/online` gives the current list, in the kernel's
comma-and-dash format:

```text
0-31          every CPU from 0 to 31
0-3,8-11      two ranges — CPUs 4-7 are offline or absent
```

**Gaps are normal, not an anomaly**: offlining a CPU, hotplug on a virtual
machine, and some firmware configurations all produce non-contiguous lists. The
parser handles them, and a fallback derives the list from `/proc/stat`'s `cpuN`
lines when `online` cannot be read.

### No subprocess

Everything is read with `std::fs`. No `lscpu`, no `cat`, no `grep`, no
`cpupower`, no shell. Spawning a process per metric would be slower, would
depend on tools that may not be installed, would parse output that changes with
locale and version, and would give PULSE a shell-injection surface it has no
reason to have.

### No privileges

Every file used is world-readable on a stock Fedora system.

## Windows

### Topology — `GetLogicalProcessorInformationEx`

Returns a packed, variable-length list of relation records. PULSE reads two
kinds and ignores the rest:

- `RelationProcessorCore` — **one record per physical core**, carrying the
  affinity mask of the logical processors on it.
- `RelationProcessorPackage` — one record per package.

Counting _records_ gives physical cores; counting _set bits_ gives logical
processors. Conflating the two is the classic Windows topology bug.

### Processor groups, and the ordinal mapping

Windows addresses logical processors as `(group, index in group)`, at most 64
per group. A machine with more than 64 logical processors has several groups,
and there is no system-wide flat processor number: CPU 70 is "group 1, bit 5".

PULSE will not put that on screen. It assigns a **PULSE ordinal** by sorting
every discovered `(group, index)` pair — group first, then index — and numbering
from zero:

```text
(0,  0) → cpu:logical-0
(0, 63) → cpu:logical-63
(1,  0) → cpu:logical-64
(1,  1) → cpu:logical-65
```

On the single-group machines that are the overwhelming majority this is the
identity mapping, so `cpu:logical-5` is Task Manager's _CPU 5_. The sort makes
the mapping independent of the order Windows returned the records in, so it
cannot change between two runs and silently re-point saved references. The map
is held for the provider's lifetime, so topology, usage and frequency all
resolve to the same `SourceId`.

**The implementation is not capped at 64 processors**, and tests cover 128- and
256-processor layouts across two and four groups.

### Per-logical usage — the API decision

This is the one place where the easy API is not the right one. Requirements:
native, unprivileged, no subprocess, no localised strings, one call per sample
rather than one per processor, correct identification of each processor,
correct beyond 64 processors, and testable arithmetic.

| Option                                                                        | Verdict                                                                                                                                                                                                                               |
| ----------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `GetSystemTimes`                                                              | Machine-wide only — no per-processor breakdown. **Kept for `cpu.usage.total`.**                                                                                                                                                       |
| PDH `\Processor(_Total)\% Processor Time`                                     | Counter paths are **localised** (`Processeur` on a French install). `PdhLookupPerfNameByIndex` works around it but adds a registry name lookup and a handle lifecycle. The legacy `Processor` object is also capped at 64 processors. |
| PDH `\Processor Information(*)\% Processor Time`                              | Group-aware, but still localised, still a heavier object model, and instance names must be string-parsed to attribute a reading to a processor.                                                                                       |
| WMI `Win32_PerfFormattedData_*`                                               | Needs the WMI service, tens of milliseconds per query. Out of scope.                                                                                                                                                                  |
| `Get-Counter`, `typeperf`, `wmic`                                             | Subprocesses. Forbidden.                                                                                                                                                                                                              |
| `QueryIdleProcessorCycleTime`                                                 | Cycle counts, not time. On a hybrid CPU the cycle rate differs between P- and E-cores, so the ratio is not a utilisation percentage.                                                                                                  |
| **`NtQuerySystemInformationEx` with `SystemProcessorPerformanceInformation`** | **Chosen.**                                                                                                                                                                                                                           |

**Why:** it returns a flat array of `SYSTEM_PROCESSOR_PERFORMANCE_INFORMATION`,
one entry per logical processor, carrying exactly the `IdleTime`, `KernelTime`
and `UserTime` totals the shared delta model already consumes. One call per
processor group covers the machine. No strings anywhere, so nothing depends on
the system language. No privileges. And the numbers are the ones Task Manager
and Process Explorer show, which is what a user will compare PULSE against.

The arithmetic is identical to `GetSystemTimes`', so `cpu.usage.total` and
`cpu.usage.logical` are the same measurement at two scopes rather than two
definitions that happen to share a name. A test asserts the two conversions
agree.

`NtQuerySystemInformationEx` is used rather than the plain form because the
plain form reports only the **calling thread's** processor group — on a
128-processor machine it silently returns 64 entries.

### An optional capability, resolved at runtime

`NtQuerySystemInformation(Ex)` lives in `ntdll`, and Microsoft documents that
layer as "may be altered or unavailable in future versions". In practice
`SystemProcessorPerformanceInformation` has been stable since Windows NT and is
what every Windows system monitor uses — but **PULSE does not treat it as a
guarantee of Windows**, and nothing in this document should be read as claiming
it is.

The decisive consequence is _how_ the symbol is bound. Declaring it in an
`extern` block would make it a **load-time import**: the Windows loader resolves
every imported symbol before the first line of PULSE runs, so on a Windows
build, emulation layer or hardened environment that does not export it, the
process would fail to start — with a loader error, before the availability model
could say anything at all. That inverts the principle the model exists to
enforce:

```text
a capability being unavailable  ≠  the application being unable to start
```

So PULSE resolves the entry point **at runtime**, once, during provider
construction:

```text
GetModuleHandleW("ntdll.dll")   ntdll is already mapped into every Win32
        │                       process — no filesystem access, no DLL search
        │                       path to hijack, no arbitrary library loaded
GetProcAddress(…, "NtQuerySystemInformationEx")
        │
probe: one undersized query     proves the symbol exists *and* understands the
        │                       information class
NtCpuApi                        a safe wrapper; the raw pointer is private and
                                never circulates through the provider
```

`GetModuleHandleW` rather than `LoadLibrary` is deliberate: `ntdll.dll` _is_ the
loader and is mapped into every Win32 process before any user code runs, so the
lookup is of a module already present rather than a request to go and find one.

If the module cannot be looked up, or the symbol is absent, or the resolved
symbol refuses the probe, the result is a structured `MetricError` that becomes
`Availability::Unsupported`. There is no `unwrap` and no `expect` anywhere on
that path, and `WindowsCpuProvider` still constructs successfully.

**What a user sees when the capability is missing:**

| Family              | State                          | Why it survives                                       |
| ------------------- | ------------------------------ | ----------------------------------------------------- |
| `cpu.usage.total`   | **Available**                  | `GetSystemTimes`, a guaranteed `kernel32` export      |
| `cpu.count.*`       | **Available**                  | `GetLogicalProcessorInformationEx`, `kernel32`        |
| `cpu.frequency.*`   | **Available**                  | `CallNtPowerInformation`, `powrprof`                  |
| `memory.*`          | **Available**                  | `GlobalMemoryStatusEx`, a different provider entirely |
| `cpu.usage.logical` | **Unsupported**, with a reason | the one casualty                                      |

This is also why `cpu.usage.total` is read from `GetSystemTimes` rather than
derived by summing the per-processor array: keeping the aggregate on an
independent, fully documented API is what lets it survive the optional one
being absent. The two are different code paths, and a test asserts their
arithmetic agrees so they cannot drift.

The per-processor metrics **stay in the catalog** either way, carrying an
unavailable status rather than disappearing — so a dashboard holding
`cpu.usage.logical@cpu:logical-3` keeps exactly the same reference on a machine
where the capability is missing, and starts working again on one where it is
not.

Resolution happens once per provider, never per refresh and never per
processor, and the resolved handle is an ordinary immutable field — no lazy
global, no mutable static.

### Windows CPU time arithmetic

**Windows counts idle time inside `kernel`**, which is the trap: the Linux
formula would report a completely idle processor as heavily busy.

```text
total = kernel + user        (kernel already contains idle)
busy  = total - idle
```

### Frequency — `CallNtPowerInformation(ProcessorInformation)`

Fills an array of `PROCESSOR_POWER_INFORMATION`, one entry per logical
processor, with `MaxMhz` and `CurrentMhz`. Documented, unprivileged, and
returns the **whole array in one call**.

Entries are attributed by **array position**, not by the struct's `Number`
field: `Number` repeats per group on multi-group systems. Position matches the
system order Windows fills — group 0's processors, then group 1's — which is the
same order the ordinal mapping uses.

`MaxMhz` on Windows is typically the processor's **base** frequency rather than
its maximum turbo frequency — the same figure the System control panel shows.
It is a hardware figure rather than a power-policy ceiling (that is `MhzLimit`,
which PULSE does not publish), so it is published as `cpu.frequency.max` with
that meaning documented rather than silently equated to the Linux figure.

Rejected alternatives: `Win32_Processor.CurrentClockSpeed` (WMI service,
machine-wide rather than per processor, notoriously stale); PDH's
`% Processor Performance` (a percentage of base clock, not a frequency, and
localised); reading MSRs directly (kernel driver and administrator).

### No privileges

All four APIs are available to any process. PULSE never requests elevation.

## The unit contract: hertz, always

Linux CPUFreq reports kilohertz. Windows' power API reports megahertz. **Both
are converted in the platform layer**, so the contract carries hertz and no
frontend ever learns that kHz exists.

```text
/sys/.../scaling_cur_freq   3 200 000 kHz  ─┐
                                             ├─► 3 200 000 000 Hz
CallNtPowerInformation      3 200 MHz      ─┘
```

Conversions are checked for overflow. **Zero is rejected**: both platforms use
`0` to mean "no figure available" — an idle-state artefact on Linux, an
unpopulated field under some hypervisors on Windows — and publishing `0 Hz`
would render as `0 GHz`, which a user reads as a claim that the core has
stopped. PULSE reports the metric unavailable instead.

Display conversion happens once, at the edge, in `formatHertz`:

```text
800_000_000   → 800 MHz
3_200_000_000 → 3.20 GHz
5_400_000_000 → 5.40 GHz
```

## What `cpu.frequency.current` actually means

> The frequency the operating system interface PULSE reads currently reports
> for this logical processor.

It is deliberately **not** claimed to be a perfect instantaneous measurement of
the silicon. Modern CPUs make that claim indefensible:

- frequency scaling moves the clock continuously between samples;
- Turbo/boost states are brief and opportunistic;
- energy policies cap the clock independently of load;
- hybrid CPUs run P-cores and E-cores at unrelated frequencies;
- under virtualisation the guest may see only the host's rated clock.

PULSE reports what the OS reports, and says so. That is an answerable promise;
"the true clock speed right now" is not.

## Hybrid CPUs

Each `cpu.frequency.max@cpu:logical-N` is an independent metric on an
independent source, so nothing in the contract forces P-cores and E-cores to
agree — and on the development machine they correctly do not.

PULSE does **not** yet label a processor as P-core or E-core in the interface.
The topology data would support a heuristic (differing maximum frequencies, or
Windows' `EfficiencyClass`), but doing it properly means handling machines where
the signal is absent or ambiguous, and that is not a Phase 3 requirement.

## Per-metric availability

Availability is carried **per metric, per processor**, never per provider:

- A machine where `cpu17` has no `cpufreq` directory still publishes `cpu17`'s
  usage. One missing file costs one metric, not thirty-one working ones.
- A frequency API failing does not affect `cpu.usage.total`,
  `cpu.usage.logical` or `cpu.count.*`.
- On Windows, the per-processor counter call failing leaves `cpu.usage.total`
  working, because it comes from a different API.

A count the platform genuinely cannot determine is published as `notDetected`
with a reason — never guessed by dividing the logical count by two.

## The CPU usage tracker

Usage is a rate, so it needs a baseline. One tracker holds the aggregate and
every logical processor behind a **single** `Mutex`:

```text
Mutex<Baselines> {
    total:   Option<CpuCounters>,
    logical: BTreeMap<LogicalId, CpuCounters>,
}
```

A mutex per processor would be 32 or 128 locks acquired in sequence on every
request, for state that is only ever written as one consistent set derived from
one system read — more machinery, more interleavings, and no contention removed,
since a request touches all of them anyway.

Every case below reports honestly and **re-primes the baseline**, so the next
request succeeds. None of them ever reports `0%`:

| Situation                   | Reported as                                           |
| --------------------------- | ----------------------------------------------------- |
| First sample, no baseline   | `temporarilyUnavailable`                              |
| Counters did not advance    | `temporarilyUnavailable`                              |
| Counters went backwards     | `temporarilyUnavailable`, baseline reset              |
| Counters self-contradictory | `temporarilyUnavailable`, reading discarded           |
| Processor newly appeared    | `temporarilyUnavailable` (no baseline yet)            |
| Processor disappeared       | absent from the report; its stale baseline is dropped |

Dropping the baseline of a departed processor matters: a CPU that goes offline
and comes back has reset counters, and differencing against pre-offline values
would produce a bogus figure.

## Performance

**One refresh of every CPU metric on Fedora costs:**

| Reads              | Count                 |
| ------------------ | --------------------- |
| `/proc/stat`       | 1                     |
| `scaling_cur_freq` | 1 per processor       |
| `cpuinfo_max_freq` | 0 — cached at startup |
| topology files     | 0 — cached at startup |

So 33 small reads on a 32-thread machine, not 32 × 4. Topology and hardware
maxima are static for the life of the process and are read once at construction.

**On Windows:**

| Call                               | Count                 |
| ---------------------------------- | --------------------- |
| `GetSystemTimes`                   | 1                     |
| `NtQuerySystemInformationEx`       | 1 per processor group |
| `CallNtPowerInformation`           | 1 (whole array)       |
| `GetLogicalProcessorInformationEx` | 0 — cached at startup |

Three calls on a typical machine. Each API is called only when a metric it
serves was actually requested.

## One CPU provider, whatever it reads

One CPU provider owns **every** CPU metric on the machine; there is no provider
per processor, and none per data source. Thirty-two providers would each re-read
`/proc/stat`, each appear in the engine status, and share nothing — and a
separate `linux.hwmon` provider would claim the same `cpu:package-N` sources the
CPU provider already owns, which the engine rejects by design.

```text
Fedora:   linux.cpu     linux.memory     linux.gpu
Windows:  windows.cpu   windows.memory   windows.gpu
```

`Providers = 3` on both platforms since Phase 4, whatever the hardware. The
**metric count** is what grows.

## Frontend

The interface contains no list of processors. `discoverLogicalProcessors` reads
them back out of the catalog, and **sorts numerically by ordinal**:

The catalog arrives ordered by the backend's `(key, sourceId)` _string_
comparison, so a 32-thread machine hands the UI `logical-1`, `logical-10`,
`logical-11`, `logical-2`. Rendering that order would scramble the table.
Parsing the ordinal out and sorting on the number produces 0, 1, 2, …, 10, 11.

A processor is listed as soon as it appears in **any** per-processor metric, so
one whose frequency is unsupported still gets a row showing its usage.

The card requests only what it displays — three references per processor plus
the three counts — never the whole catalog.

### No polling

There is exactly one sample on mount and one per click of Refresh. A hidden
interval would be a scheduler in disguise, one that keeps running when the
window is hidden and that nothing else could coordinate with. A test advances
timers by sixty seconds and asserts no further request is made.

The catalog is fetched once: processors do not appear and vanish while the
window is open, and re-deriving the table's shape on every refresh would make
rows jump under the pointer for no gain.

## Still deliberately absent

No temperatures, no throttling, no power, no voltage, no fan speeds, no load
average, no per-process CPU. No scheduler, no polling, no history, no SQLite,
no graphs, no widgets, no selectors.
