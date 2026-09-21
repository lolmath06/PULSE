# PULSE on Fedora Linux

> Status: Phase 4. CPU (aggregate, per logical processor, frequency, topology),
> physical memory, and GPU (inventory, identity and core telemetry) are
> implemented natively. Everything below the
> "Planned data sources" heading is still design work.

Fedora Linux is a **first-class PULSE platform**, on equal footing with Windows.

## What PULSE reads on Fedora today

**Implemented (Phase 2).** No elevated privileges required.

| Metric                  | Source                              | Notes                                           |
| ----------------------- | ----------------------------------- | ----------------------------------------------- |
| `cpu.usage.total`       | `/proc/stat`                        | Aggregate `cpu` line; delta between two samples |
| `cpu.usage.logical`     | `/proc/stat`                        | `cpuN` lines; same formula as the aggregate     |
| `cpu.frequency.current` | `cpufreq/scaling_cur_freq`          | kHz → Hz in the platform layer                  |
| `cpu.frequency.max`     | `cpufreq/cpuinfo_max_freq`          | Hardware maximum, **not** `scaling_max_freq`    |
| `cpu.count.logical`     | `/sys/devices/system/cpu/online`    | Hardware threads                                |
| `cpu.count.physical`    | `cpuN/topology/`                    | Distinct `(package_id, core_id)` pairs          |
| `cpu.count.package`     | `cpuN/topology/physical_package_id` | Distinct packages                               |
| `memory.total`          | `/proc/meminfo`                     | `MemTotal`, kB → bytes                          |
| `memory.available`      | `/proc/meminfo`                     | `MemAvailable`, kB → bytes                      |
| `memory.used`           | derived                             | `total - available`                             |
| `memory.usage.percent`  | derived                             | `used / total * 100`                            |

| `gpu.count` | `/sys/class/drm` | Hardware adapters, excluding virtual devices |
| `gpu.usage.core` | NVML, or `gpu_busy_percent` | Per adapter; vendor-dependent |
| `gpu.memory.*` | NVML, or `mem_info_vram_{total,used}` | Dedicated VRAM only |
| `cpu.temperature.package` | `hwmon`: `coretemp` / `k10temp` / `peci_cputemp` | Millidegrees → Celsius; package channel only, never a core average |
| `gpu.temperature.core` | NVML, or the card's `hwmon` node | `amdgpu` `edge`, or a single unlabelled channel |
| `gpu.temperature.hotspot` | `amdgpu` `junction` | A separate sensor, never derived from the die temperature |
| `gpu.temperature.memory` | `amdgpu` `mem` | Likewise its own sensor |
| `gpu.fan.speed` | NVML RPM query, or `fanN_input` | RPM only; a control percentage is never republished as a speed |
| `gpu.frequency.*` | NVML, or `pp_dpm_{sclk,mclk}` | MHz → Hz in the platform layer |

The per-processor metrics exist once per logical processor and the GPU metrics
once per adapter, so the catalog holds `9 + 3N + 7G` metrics — 112 on this
32-thread, single-GPU machine.

Three details that the implementation gets right and that are easy to get wrong:

- **`iowait` counts as idle** in the CPU calculation — the CPU really is
  executing nothing while a task waits for I/O.
- **`guest` and `guest_nice` are not added to the CPU total.** The kernel
  already includes them inside `user` and `nice`; counting them twice inflates
  the total and makes a busy host look idle.
- **`MemAvailable`, never `MemFree`.** Free memory excludes reclaimable page
  cache, so a healthy machine with a warm cache would look nearly out of memory.

Values in `/proc/meminfo` are converted from kB to bytes inside the platform
layer, so nothing above it ever handles a non-canonical unit. An unexpected unit
suffix is rejected rather than guessed — a wrong guess is off by a factor of 1024.
CPUFreq's kHz figures are converted to hertz in the same place, for the same
reason.

Four more details the Phase 3 implementation gets right:

- **One `/proc/stat` read feeds every usage metric.** The file carries the
  aggregate line and every `cpuN` line, so reading it per metric would be 33
  reads instead of 1 — and would sample processors at different instants, so
  they would not reconcile with the aggregate.
- **`cpuinfo_max_freq`, never `scaling_max_freq`.** The latter is the current
  power-policy ceiling: a laptop in a power-saving profile reports a scaling
  maximum far below what the chip can do. When the hardware figure is absent,
  PULSE reports the metric `unsupported` rather than substituting the policy value.
- **Gaps in the CPU list are normal.** `/sys/devices/system/cpu/online` can read
  `0-3,8-11` after a CPU is offlined or hot-unplugged. The parser handles ranges,
  gaps and a missing file (falling back to `/proc/stat`'s `cpuN` lines).
- **Physical cores are counted as `(package_id, core_id)` pairs.** `core_id` is
  only unique within a package, so counting it bare would report a dual-socket
  32-core server as having 32 cores.

**No subprocess is ever spawned.** No `lscpu`, no `cat`, no `grep`, no
`cpupower`, and for GPUs no `nvidia-smi`, `lspci`, `glxinfo`, `vulkaninfo`,
`radeontop`, `rocm-smi` or `intel_gpu_top` — everything is read with `std::fs`
or through a library loaded at runtime. Spawning a process per metric
would be slower, would depend on tools that may not be installed, would parse
output that changes with locale, and would give PULSE a shell-injection surface.

### GPU

`/sys/class/drm` is the generic inventory: only `card<N>` entries that resolve
to a real PCI device count, so display connectors (`card0-DP-1`), render nodes
(`renderD128`) and virtual devices (`vkms`) are excluded. Device names come from
the system PCI ID database (`/usr/share/hwdata/pci.ids`), read as an ordinary
file — the same data `lspci` prints, without running it.

Telemetry comes from whichever backend serves the card: **NVML** for NVIDIA
(loaded at runtime via `dlopen("libnvidia-ml.so.1")`, absent without the
proprietary driver), or the **`amdgpu`** driver's sysfs attributes. A card
served by neither — an NVIDIA GPU running `nouveau`, for instance — is still
inventoried, named and identified, with its performance metrics honestly
`unsupported`. It never disappears.

A card served by no telemetry backend still reports its temperature when it has
a sensor: the performance and thermal halves are independent, and the interface
says so rather than describing the whole GPU as unavailable.

Full formulas and edge cases:
[`../metrics/cpu-memory.md`](../metrics/cpu-memory.md),
[`../metrics/cpu-advanced.md`](../metrics/cpu-advanced.md),
[`../metrics/gpu.md`](../metrics/gpu.md) and
[`../metrics/thermals.md`](../metrics/thermals.md).

## What Phase 0 already does on Fedora

- Reads `PRETTY_NAME` from `/etc/os-release` (falling back to
  `/usr/lib/os-release`) to report the distribution.
- Detects the display server (`XDG_SESSION_TYPE`, then `WAYLAND_DISPLAY` /
  `DISPLAY`) and reports `wayland` or `x11`.
- Both live in `src-tauri/src/platform/linux/`, compiled only on Linux.

## Development prerequisites

Tauri 2 needs the WebKitGTK 4.1 stack:

```bash
sudo dnf install -y \
  webkit2gtk4.1-devel \
  openssl-devel \
  curl wget file \
  libappindicator-gtk3-devel \
  librsvg2-devel \
  gcc gcc-c++ make
```

Plus Rust (`rustup`) and Node.js 20.19+ with pnpm.

## Planned data sources

None of this is implemented yet; it is the map for later phases.

### CPU

- `/proc/stat` — per-core jiffies. The aggregate line is already implemented;
  per-core utilisation is the same file and the same delta arithmetic.
- `/proc/cpuinfo` — model, core count, and `cpu MHz` (per-core, instantaneous).
- `/sys/devices/system/cpu/cpu*/cpufreq/scaling_cur_freq` — more reliable
  current frequency than `/proc/cpuinfo`.
- `/proc/loadavg` — load averages.

### Memory

- `/proc/meminfo` — the authoritative source, **implemented**. "Used" is
  `MemTotal - MemAvailable`, **not** `MemTotal - MemFree`; the naïve formula is
  a classic and very visible bug.
- Swap (`SwapTotal`, `SwapFree`) is a later phase.

### Storage

- `/proc/diskstats` — I/O counters, again as deltas.
- `statvfs(2)` for filesystem usage.
- SMART requires `smartctl`, which needs elevated privileges — treat as an
  optional, permission-gated capability.

### Network

- `/proc/net/dev` or `/sys/class/net/*/statistics/` — byte and packet counters.

### Temperatures and fans — `hwmon`

**Implemented for the CPU package and for GPUs** — see
[`../metrics/thermals.md`](../metrics/thermals.md) for the full mapping, the
millidegree conversion and the readings PULSE refuses.

`/sys/class/hwmon/hwmon*/` is the interface, with `name`, `tempN_input`
(millidegrees Celsius), `tempN_label`, `fanN_input` (RPM). PULSE reads only
`*_input` channels: `tempN_crit`, `tempN_max` and the `pwm*` control files are
never read, and nothing is ever written.

What is implemented, and the rules it follows:

- Sensor numbering is **not** stable across kernel versions, hardware or
  reboots, so nothing is keyed on `hwmonN`. Identity comes from the driver
  `name`, the `device` symlink's target and the channel label.
- `coretemp`'s `Package id N` maps to `cpu:package-N` using the kernel's own
  package numbering. `Core N` is never used, and never averaged into a package
  temperature.
- `k10temp` publishes only from `Tdie`; `Tctl` is an offset control value and
  `Tccd*` are chiplet sensors.
- `peci_cputemp` publishes only from `Die`; `Tjmax`, `Tthrottle`, `Tcontrol` and
  `DTS` are limits and offsets, not the current temperature.
- A GPU's sensors are found under the card's own `<device>/hwmon/`, which is what
  ties them to that card rather than to a numbering coincidence.
- A missing sensor is a normal state, not an error: the metric keeps its
  definition and says why it has no value, and never shows a zero.

**Still not implemented**: motherboard and chipset sensors, which typically need
`nct6775` or similar and often `acpi_enforce_resources=lax` — something PULSE
must never ask a user to set silently. `lm_sensors` and `sensors-detect` are not
involved at any point; PULSE reads `sysfs` directly.

### GPU

- **AMD** — `/sys/class/drm/card*/device/` exposes `gpu_busy_percent`,
  `mem_info_vram_used`, and hwmon entries for temperature and fan speed. No root
  needed. The best-supported case on Fedora. Power draw remains unread.
- **NVIDIA** — NVML via the proprietary driver. Fedora users may be on nouveau,
  which exposes far less: on the development machine's RTX 4070, `nouveau`
  publishes **no** hwmon node at all, so the card is correctly inventoried with
  every performance and thermal metric honestly unsupported. PULSE detects and
  degrades rather than assuming.
- **Intel** — `/sys/class/drm/card*/`, plus `intel_gpu_top`'s interfaces, which
  may require `CAP_PERFMON`.

## Permissions

The guiding principle: **PULSE must be useful without root.** Anything needing
elevation is an optional enhancement, clearly labelled, never a hard
requirement, and never silently requested.

Known elevation points: SMART attributes, some motherboard sensors, some Intel
GPU counters, and RAPL energy counters (`/sys/class/powercap/`, restricted since
CVE-2020-8694).

## Wayland and X11

Fedora Workstation defaults to **Wayland** (GNOME). PULSE must work well there.

For the main window this is transparent. For the future **Mini overlay** it is
the central problem: under Wayland a client cannot position its own windows or
force stacking order, and GNOME's Mutter does not implement `wlr-layer-shell`.
See [`../architecture/mini-overlay.md`](../architecture/mini-overlay.md) for the
full analysis.

X11 sessions remain common and are simpler for overlays; PULSE should support
them where it costs little, without treating X11 as the target.

## Packaging

Planned targets: `rpm` (primary for Fedora), plus `deb` and `appimage` for
reach. Configured in `tauri.conf.json`; installer production is a later phase.

## Testing note

CI runs on `ubuntu-latest`. That catches Linux regressions — wrong `cfg` gating,
missing Linux paths, build breakage — but **it is not a Fedora test**. Different
kernel, different WebKitGTK, different GNOME, different sensor modules. Real
Fedora validation is manual and is required before any system-facing feature is
considered complete.
