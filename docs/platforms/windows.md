# PULSE on Windows

> Status: Phase 0. This documents what PULSE _will_ rely on and the constraints
> that come with it. No metric integration is implemented yet.

Windows is a **first-class PULSE platform**, on equal footing with Fedora Linux.

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

- `GetSystemTimes` / `NtQuerySystemInformation` for utilisation, as deltas.
- PDH `\Processor Information(*)\% Processor Utility` for per-core detail.
- Frequency is awkward: `\Processor Information(*)\% Processor Performance`
  scaled by the nominal frequency is the usual approach.

### Memory

- `GlobalMemoryStatusEx` for totals, `GetPerformanceInfo` for detail.

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
