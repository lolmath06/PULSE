# PULSE on Fedora Linux

> Status: Phase 0. This documents what PULSE _will_ rely on and the constraints
> that come with it. No metric integration is implemented yet.

Fedora Linux is a **first-class PULSE platform**, on equal footing with Windows.

## What PULSE reads on Fedora today

**Implemented (Phase 2).** No elevated privileges required.

| Metric                 | File            | Notes                                           |
| ---------------------- | --------------- | ----------------------------------------------- |
| `cpu.usage.total`      | `/proc/stat`    | Aggregate `cpu` line; delta between two samples |
| `memory.total`         | `/proc/meminfo` | `MemTotal`, kB → bytes                          |
| `memory.available`     | `/proc/meminfo` | `MemAvailable`, kB → bytes                      |
| `memory.used`          | derived         | `total - available`                             |
| `memory.usage.percent` | derived         | `used / total * 100`                            |

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

Full formulas and edge cases: [`../metrics/cpu-memory.md`](../metrics/cpu-memory.md).

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

`/sys/class/hwmon/hwmon*/` is the interface, with `name`, `tempN_input`
(millidegrees Celsius), `tempN_label`, `fanN_input` (RPM).

This is the messiest area on Linux and deserves care:

- Sensor naming is **not** stable across kernel versions, hardware, or even
  reboots. Never key configuration on `hwmonN`; key it on the `name` attribute
  plus the label.
- Which sensors exist depends on loaded kernel modules. `coretemp` (Intel) and
  `k10temp` (AMD) are usually present; motherboard sensors typically need
  `nct6775` or similar, often requiring `acpi_enforce_resources=lax`, which PULSE
  must **never** ask users to set silently.
- `lm_sensors` may need `sensors-detect` to have been run.
- A missing sensor is a normal state, not an error. The UI must say
  "not available on this system" rather than showing zero.

### GPU

- **AMD** — `/sys/class/drm/card*/device/` exposes `gpu_busy_percent`,
  `mem_info_vram_used`, and hwmon entries for temperature and power. No root
  needed. The best-supported case on Fedora.
- **NVIDIA** — NVML via the proprietary driver. Fedora users may be on nouveau,
  which exposes far less. PULSE must detect and degrade rather than assume.
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
