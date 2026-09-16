# PULSE on Windows

> Status: Phase 3. CPU (aggregate, per logical processor, frequency, topology)
> and physical memory are implemented natively. Everything below the
> "Planned data sources" heading is still design work.

Windows is a **first-class PULSE platform**, on equal footing with Fedora Linux.

## What PULSE reads on Windows today

**Implemented (Phase 2).** No administrator rights required.

| Metric                  | API                                                                    | Notes                                  |
| ----------------------- | ---------------------------------------------------------------------- | -------------------------------------- |
| `cpu.usage.total`       | `GetSystemTimes`                                                       | Delta between two samples              |
| `cpu.usage.logical`     | `NtQuerySystemInformationEx` / `SystemProcessorPerformanceInformation` | One call per processor group           |
| `cpu.frequency.current` | `CallNtPowerInformation(ProcessorInformation)`                         | `CurrentMhz`, MHz → Hz                 |
| `cpu.frequency.max`     | `CallNtPowerInformation(ProcessorInformation)`                         | `MaxMhz`, typically the **base** clock |
| `cpu.count.logical`     | `GetLogicalProcessorInformationEx`                                     | Set bits across core records           |
| `cpu.count.physical`    | `GetLogicalProcessorInformationEx`                                     | `RelationProcessorCore` **records**    |
| `cpu.count.package`     | `GetLogicalProcessorInformationEx`                                     | `RelationProcessorPackage` records     |
| `memory.total`          | `GlobalMemoryStatusEx`                                                 | `ullTotalPhys`                         |
| `memory.available`      | `GlobalMemoryStatusEx`                                                 | `ullAvailPhys`                         |
| `memory.used`           | derived                                                                | `total - available`                    |
| `memory.usage.percent`  | derived                                                                | `used / total * 100`                   |

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
consumes, with no strings anywhere, in one call per processor group. Its cost is
that `ntdll` is documented as subject to change; PULSE contains that by
isolating the call, degrading to "unavailable" rather than panicking, and
keeping `cpu.usage.total` on the fully documented `GetSystemTimes`. The full
comparison is in [`../metrics/cpu-advanced.md`](../metrics/cpu-advanced.md).

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
that declare the calls PULSE makes. `NtQuerySystemInformationEx` and
`PROCESSOR_POWER_INFORMATION` are not shipped by `windows-sys` and are declared
directly, next to the code that uses them.

Every `unsafe` block is a handful of lines wrapping one API call, each with a
`# Safety` comment explaining why it is sound, and each converting the packed
Windows buffer into plain Rust structs immediately. Everything after that —
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

### Storage and network

- PDH counters (`\LogicalDisk(*)`, `\Network Interface(*)`), plus
  `GetDiskFreeSpaceEx` for capacity.
- SMART requires `DeviceIoControl` with administrator rights.

### Temperatures and sensors — the hard part

Windows has **no general, unprivileged temperature API**.

- `MSAcpi_ThermalZoneTemperature` (WMI) exists but is frequently unimplemented
  by OEM firmware, and reports an ACPI thermal zone rather than a CPU package
  temperature.
- Real CPU temperature means reading MSRs; real motherboard sensors mean talking
  to a Super I/O chip. Both need ring-0 access, i.e. a signed kernel driver.
- Tools like HWiNFO and LibreHardwareMonitor ship exactly such a driver.

**This is a deliberate open decision for PULSE**, not an oversight. Shipping a
kernel driver brings signing costs, security exposure and a support burden. The
alternatives are to integrate with an existing tool's shared-memory interface
(e.g. HWiNFO's, which is opt-in and user-installed), or to expose vendor SDK
temperatures only (NVML for NVIDIA, ADLX for AMD) and be explicit that
motherboard sensors are unavailable.

Until that decision is made, Windows temperature support must be treated as
`Unavailable(reason)` rather than quietly showing zeros.

### GPU

- **NVIDIA** — NVML, the most complete source.
- **AMD** — ADLX / ADL.
- **Intel** — Intel Power Gadget's successors, or DXGI.
- **Vendor-neutral baseline** — DXGI adapter info plus the
  `\GPU Engine(*)\Utilization Percentage` PDH counter set, which needs no vendor
  SDK and works for basic utilisation and VRAM.

## Privileges

Same principle as Fedora: **PULSE must be useful without administrator
rights.** Elevation-requiring data (SMART, MSR-based temperatures, some PDH
counter sets) is an optional, clearly labelled enhancement.

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

## Packaging

Planned targets: NSIS (`.exe`) and MSI, configured in `tauri.conf.json`.

One caveat already worth recording: PULSE's version is `0.1.0-dev`, and **MSI
requires a strictly numeric `major.minor.patch` version**. The pre-release
suffix must be dropped or translated before the first MSI is produced. Installer
production is a later phase, so this is documented rather than solved now.

## Testing note

CI builds and tests on `windows-latest`, which catches compilation and
unit-test regressions on the real MSVC toolchain. That is genuine automated
coverage, but it is not a substitute for launching PULSE on a physical Windows
machine and looking at it. Both are required before a system-facing feature is
considered complete.
