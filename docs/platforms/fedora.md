# PULSE on Fedora Linux

> Status: Phase 8. CPU (aggregate, per logical processor, frequency, topology),
> physical memory, GPU, thermals, storage, networking and processes are
> implemented natively, and validated on real hardware. Everything below the
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
| `storage.device.count` | `/sys/class/block` | Physical disks; loop, zram, dm and partitions excluded |
| `storage.volume.count` | `/proc/self/mountinfo` | One per filesystem, however many paths it is mounted at |
| `storage.capacity.total` | `/sys/block/<dev>/size` | **512-byte sectors**, never scaled by the logical block size |
| `storage.io.*` | `/proc/diskstats` | One read for every device; delta between two samples |
| `storage.health.temperature` | `nvme` `hwmon` `Composite`, else the health log | Found by label, never by taking `temp1_input` positionally |
| `storage.health.*` (other) | `NVME_IOCTL_ADMIN_CMD`, log page `0x02` | Needs privilege; `permissionDenied` when refused |
| `storage.volume.capacity.*` | `statvfs(3)` | `used = total - free`, `available = user-available` |
| `network.interface.count` | `rtnetlink` `RTM_GETLINK` | Loopback excluded; every other interface published |
| `network.receive.*`, `network.transmit.*` | `IFLA_STATS64`, in the same dump | Delta between two samples; errors and drops are separate counters |
| `network.link.*_speed` | `/sys/class/net/<iface>/speed` | Megabits → bits; `-1` and `EINVAL` mean unknown, never zero |
| `network.mtu` | `IFLA_MTU` | |
| `network.wifi.signal.rssi` | `nl80211` `NL80211_STA_INFO_SIGNAL` | A **signed** byte in dBm; strongest link when several |
| `network.wifi.signal.quality` | — | **Unsupported**: `cfg80211` reports dBm and no percentage, and PULSE does not invent one |
| `network.wifi.link.*_rate` | `nl80211` `RATE_INFO_BITRATE32` | Units of 100 kbit/s → bits |

The per-processor metrics exist once per logical processor, the GPU metrics once
per adapter, and the storage metrics once per device and per volume, so the
catalog holds `13 + 3N + P + 11G + 13D + 4V + 11I + 4W` metrics — 307 on this
32-thread, single-GPU machine with two disks, six mounted filesystems, twelve
published network interfaces and one Wi-Fi radio.

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

### Storage — implemented in Phase 6

- `/sys/class/block` — the physical inventory: capacity, WWID, serial, model,
  rotational and removable flags, and the device chain the bus is derived from.
- `/proc/diskstats` — I/O counters, as deltas, **one read for every device** so
  their intervals coincide.
- `/proc/self/mountinfo` — volumes, rather than `/etc/mtab`: it carries the
  device number that makes two mounts of one filesystem recognisable as one
  filesystem, the root within the filesystem that tells a bind mount from a
  separate volume, and octal-escaped paths that a whitespace split would cut in
  half.
- `statvfs(3)` for filesystem usage, via `libc` — never by running `df`.
- NVMe health through the kernel's `nvme` `hwmon` node (composite temperature,
  unprivileged) and `NVME_IOCTL_ADMIN_CMD` `Get Log Page 0x02` (the rest,
  privileged). **No `smartctl`, no `nvme`, no subprocess of any kind.**
- ATA SMART is deliberately **deferred**: its attributes are vendor-defined, and
  normalising them wrongly would publish confident but false claims about a
  user's disk. See [`../metrics/storage.md`](../metrics/storage.md#health-nvme-only-for-now).

### Network — implemented in Phase 7

- **`rtnetlink` `RTM_GETLINK`** — the whole interface inventory _and_ every
  counter in one transaction: name, MTU, operational state, both hardware
  addresses, the virtual device kind, and `IFLA_STATS64`. Not
  `/sys/class/net/*/statistics/`, which is one file per counter per interface:
  on this machine that is 104 file opens at 104 slightly different instants,
  and rates need one instant.
- **`IFLA_PERM_ADDRESS`** for identity. It is what survives MAC randomisation,
  which NetworkManager does per network by default, and what makes an
  interface's `SourceId` match the one Windows derives for the same card.
- **`rtnetlink` `RTM_GETADDR`** — local addresses, for display only.
- **`nl80211`** — which interfaces are actually wireless, and their signal and
  negotiated rates. Asking the family is the only reliable way: every Wi-Fi
  station reports `ARPHRD_ETHER` exactly like a wired NIC, and `wlan0` is a
  naming convention nothing enforces.
- **`/sys/class/net/<iface>/speed`** — Ethernet link speed, one small read per
  wired interface. The structured alternative, `ETHTOOL_MSG_LINKMODES_GET`,
  would mean a third netlink family and a third attribute mapping for one
  number.
- **No `ip`, `ifconfig`, `ethtool`, `iw`, `iwconfig`, `nmcli` or `networkctl`**,
  and no subprocess of any kind.

The netlink framing is parsed in PULSE rather than through a crate, so every
bound check — truncated headers, attributes claiming more than the buffer
holds, the zero-length attribute that hangs a naive walker — is a unit test
rather than a hope. See
[`../metrics/network.md`](../metrics/network.md#parsing-netlink-safely).

### Processes — implemented in Phase 8

Read directly from `/proc`. **No `ps`, `top`, `htop`, `pidstat`, `pgrep` or
`cat`, and no subprocess of any kind** — each of those is a program that opens
the same files PULSE opens, formats the result for a terminal, and hands back
locale-dependent text that then has to be parsed back into numbers.

Per process, at most:

| Read                | Yields                                                        |
| ------------------- | ------------------------------------------------------------- |
| `/proc/<pid>/stat`  | name, state, parent, thread count, CPU time, RSS, start token |
| `/proc/<pid>/io`    | `read_bytes` / `write_bytes` — block-device traffic only      |
| `/proc/<pid>/exe`   | the executable, for grouping processes into applications      |
| `stat(/proc/<pid>)` | the owning user, for classification                           |

plus one `read_dir` of `/proc`, one `/proc/meminfo` and one `/proc/stat` per
pass. `/proc/<pid>/statm` and `/proc/<pid>/status` are deliberately **not**
read: `stat` already carries the resident set and the thread count, and adding
them would be several hundred more opens per refresh for numbers PULSE already
has.

**Measured on this machine: 702 processes, 2 164 threads, 16–26 ms per
snapshot.** The walk is sequential; the snapshot reports its own duration so
the case for parallelism can be made from a measurement rather than a guess.

Two things about `/proc` that this code exists to get right:

- **`/proc/<pid>/stat` field 2 is not escaped.** `comm` is wrapped in
  parentheses and may contain spaces and parentheses of its own — `Web
Content`, `foo (bar)`, `kworker/3:1H-events` are all real. A naive
  `split_whitespace` shifts every later field, which does not fail: it
  succeeds, with the wrong columns, giving a `starttime` that changes every
  refresh and a CPU column permanently blank for exactly the processes the user
  cares about. PULSE splits on the **last** `)`.
- **Processes vanish while you look at them.** `ENOENT` between `read_dir` and
  any open, or between two opens, is expected and costs that one process's row.
  It is not a provider failure and never a panic.

Kernel threads are identified from the kernel's own `PF_KTHREAD` flag in field
9, not from a bracketed name or a zero resident set — a user program may
legitimately have both.

See [`../metrics/processes.md`](../metrics/processes.md) for the CPU
normalisation, the PID-reuse identity and the application grouping.

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

Known elevation points: the NVMe health log, some motherboard sensors, some
Intel GPU counters, and RAPL energy counters (`/sys/class/powercap/`, restricted
since CVE-2020-8694).

Storage is the worked example of the principle. `/dev/nvme0` is
`crw------- root:root` on Fedora, so an unprivileged PULSE reports:

| Value                              | Unprivileged | Source                                                |
| ---------------------------------- | ------------ | ----------------------------------------------------- |
| Inventory, capacity, bus, identity | yes          | `/sys/class/block`                                    |
| Volumes and their usage            | yes          | `mountinfo`, `statvfs`                                |
| I/O counters                       | yes          | `/proc/diskstats`                                     |
| `storage.health.temperature`       | yes          | the `nvme` `hwmon` node                               |
| The other five health values       | **no**       | `permissionDenied`, with a reason naming `/dev/nvme0` |

Everything else keeps working, and the five refused values say the OS refused
rather than claiming the drive has no health data — a distinction the
availability contract exists to preserve.

Networking needs no privilege at all: `AF_NETLINK` is open to any process,
`RTM_GETLINK` and `RTM_GETADDR` are unprivileged dumps, `nl80211`'s station
query works for any user, and `/sys/class/net` is world-readable.

Processes are the second worked example, and the degradation is per field
rather than per process. `/proc/<pid>/stat` is world-readable, so every process
on the machine is listed with its name, PID, parent, state, thread count, CPU
time and resident memory. `/proc/<pid>/io` and `/proc/<pid>/exe` are owned by
the process's own user, so another user's processes lose exactly those two:

| Value                             | Another user's process | Source             |
| --------------------------------- | ---------------------- | ------------------ |
| Name, PID, parent, state, threads | yes                    | `/proc/<pid>/stat` |
| CPU time and resident memory      | yes                    | `/proc/<pid>/stat` |
| Read and write rates              | **no**                 | `permissionDenied` |
| Executable path                   | **no**                 | `permissionDenied` |

On this machine that is 125 of 702 processes, every one of which keeps its
other columns. The row is never dropped, and the two refused cells never become
`0 B/s`.

## Wayland and X11

Fedora Workstation defaults to **Wayland** (GNOME). PULSE must work well there.

For the main window this is transparent. For the future **Mini overlay** it is
the central problem: under Wayland a client cannot position its own windows or
force stacking order, and GNOME's Mutter does not implement `wlr-layer-shell`.
See [`../architecture/mini-overlay.md`](../architecture/mini-overlay.md) for the
full analysis.

X11 sessions remain common and are simpler for overlays; PULSE should support
them where it costs little, without treating X11 as the target.

## Process inspector and controls (Phase 9)

| Need            | Source                                                                  |
| --------------- | ----------------------------------------------------------------------- |
| identity, state | `/proc/<pid>/stat` (start ticks = identity token)                       |
| owner           | `/proc/<pid>/status` real UID → `getpwuid_r`                            |
| start time      | `/proc/stat` `btime` + start ticks ÷ `_SC_CLK_TCK`                      |
| executable      | `readlink`/`stat` of `/proc/<pid>/exe` (`(deleted)` detected)           |
| architecture    | ELF header (20 bytes) of `/proc/<pid>/exe`                              |
| package         | `rpm -qf -- <path>` — no shell, 5 s timeout, lazy                       |
| priority        | `getpriority` / `setpriority` per thread                                |
| affinity        | `sched_getaffinity` / `sched_setaffinity` per thread                    |
| pin + signals   | `pidfd_open` + `pidfd_send_signal` (SIGTERM, SIGKILL, SIGSTOP, SIGCONT) |

Everything runs with the user's own permissions: other users' processes and
kernel threads are shown with the reason actions are unavailable; lowering a
nice value is normally `permissionDenied`. No sudo, no pkexec. Validated on this
machine with a `sleep` child the tests spawn themselves (see the Phase 9 report
and `platform/linux/processes/control/runtime_tests.rs`). Details:
`docs/processes/`.

## History and visualization (Phase 10)

- **Database:** `~/.local/share/dev.pulse.app/history.sqlite3` (Tauri's
  `app_local_data_dir`, i.e. `$XDG_DATA_HOME/dev.pulse.app`). SQLite is compiled
  in (`rusqlite` `bundled`), so no `sqlite` RPM is required.
- **Validated physically on Fedora 39** (debug build, 32 logical processors):
  87 historized references, 79 producing values; batches every 5 000 ms
  (median; 4 975–5 020 ms observed), 79 rows per batch, sampling ≈ 15 ms median
  and insert ≈ 0.31 ms median; WAL mode, `synchronous = NORMAL`. Across a close
  and relaunch the history persisted, the only interval above 15 s was the
  restart itself (drawn as a gap), no batch was duplicated while every card
  sampled on mount, the WAL was checkpointed to 0 bytes on exit and no `pulse`
  process remained. The file contained no MAC, serial, interface or user name.
- **NVMe health is no longer read with I/O rates.** The storage provider now
  issues the NVMe admin command only when a `storage.health.*` metric of that
  drive is requested, so the 5-second history sampler never wakes the SSD's
  controller. Drive temperature is therefore not historized.
- **GPU without NVML** (nouveau / no proprietary driver): `gpu.usage.core` is
  unavailable, writes no rows, and the GPU history panel says _Telemetry
  unavailable_ instead of drawing zeros.

## Dashboard, widgets and overlays (Phase 11)

- **Configuration:** `~/.config/dev.pulse.app/ui-config.json` (+ `.bak`),
  atomic, shared by every window. The development build's Phase 10
  `localStorage` preferences were migrated into it on first launch (verified).
- **Session:** GNOME on Wayland. Overlay capabilities as PULSE reports and as
  measured are in
  [`../overlay/platform-capabilities.md`](../overlay/platform-capabilities.md):
  native Wayland — always-on-top _limited_, click-through _limited_,
  positioning _unsupported_, global shortcut _limited_, tray _limited_;
  XWayland — always-on-top and click-through **measured** (`_NET_WM_STATE_ABOVE`,
  1×1-pixel input region when locked).
- **Tray:** `libappindicator3` is present; GNOME shows tray icons only with
  the AppIndicator extension.
- **Lifecycle (measured, XWayland):** closing the main window with _Quit_
  closed both overlays and exited cleanly (history stopped, WAL checkpointed to
  0 bytes, no `pulse` process). With _Keep running_, the main window hid and
  the overlays stayed; closing the overlays from the window manager hid them in
  the configuration and brought the main window back; closing it then quit.
- **Keep running — corrective (measured, release build, XWayland, 2 overlays,
  under a capped user scope):** closing the main window logged `HideMain`;
  both overlays stayed mapped and kept updating; closing the last overlay
  showed the **same** main window (same X11 window, no duplicate); closing it
  then logged `Quit`, the history recorder stopped, the WAL was 0 bytes and no
  PULSE or WebKit process remained. _Open PULSE_ on an overlay and tray Quit
  need a physical click (synthetic input does not reach PULSE here) and are
  in the manual protocol. Native Wayland, same two overlays: ~280 MB for the
  whole process tree, about 6 % of one core, stable over 50 s.
- **Physical validation (Phase 11 corrective, by the user):** dashboard, tiny
  widgets, customization, persistence, overlay layouts, several overlays,
  keep running, _Open PULSE_, click-through and Mini pass. **Always on top on
  native GNOME Wayland does not hold**: a focused application is drawn above
  the overlay — a compositor limitation PULSE does not work around. **The
  global shortcut did nothing** with Firefox focused: the plugin's X11 grab
  sits on XWayland and never sees keys while a Wayland window is focused.
- **Global shortcut backend (Wayland):** PULSE now uses the XDG Desktop Portal
  `GlobalShortcuts` interface on native Wayland. This machine's portal
  (xdg-desktop-portal 1.18.2, xdg-desktop-portal-gnome 45.1, GNOME Shell 45)
  **does not offer it** (checked read-only with `busctl --user introspect`),
  so on Fedora 39 PULSE reports the shortcut **unsupported** with that reason
  instead of claiming a shortcut that cannot fire. GNOME 48 (Fedora 42) and
  KDE Plasma ≥ 5.27 provide the interface.
- **Runtime check (release build, native Wayland, one overlay, capped
  scope):** the backend was chosen once — _unavailable_, with the portal's own
  error — no portal session was created, no X11 grab was registered, and
  PULSE → Overlays reports the shortcut unsupported. Quitting through PULSE's
  tray menu exited cleanly (exit code 0, history stopped, WAL 0 bytes, no
  process left). The portal path itself (dialog, `Activated`) needs a desktop
  that offers it and was not exercised here.
- **Manual test semantics:** click-through succeeds when the application
  behind the locked overlay receives the click _and_ the focus while the
  overlay stays drawn (it failing to stay above is always-on-top, which is
  not available on native GNOME Wayland). The global shortcut is tested from
  another focused application.
- **Cost (debug build, measured over 60 s):** 1.33 % of one core with the main
  window alone, 3.95 % with two live overlays (four live widgets).

## Packaging

Release targets are `rpm` (primary for Fedora), plus `deb` and AppImage for
reach. They are configured in `tauri.conf.json` and built by the Release job.

The AppImage is intentionally thin: it contains PULSE and its desktop assets,
but uses the target system's WebKitGTK 4.1, GTK and GLib libraries. Tauri's
default AppImage bundler copies those libraries and WebKit helper processes
from the build host. That made the package's runtime depend on the Ubuntu
runner: the Ubuntu 22.04/WebKitGTK 2.50.4 bundled stack showed sustained web
process CPU use and a startup-preset crash on the Fedora 39/nouveau validation
machine, while the earlier Ubuntu 24.04 package carried WebKitGTK 2.52.6.

Building the package on Ubuntu 24.04 is not a compatibility fix: that WebKitGTK
library itself requires GLIBC 2.38, and the PULSE executable linked there also
acquired GLIBC 2.39 symbol versions. The release therefore stays on Ubuntu
22.04 and packages no copy of the GUI runtime. Install `webkit2gtk4.1` and
`libappindicator-gtk3` on Fedora before using the AppImage, just as the RPM
declares. The release check extracts the finished AppImage, rejects bundled
shared libraries or WebKit helper processes, and checks every shipped ELF
against the GLIBC 2.35 ceiling.

### nouveau/WebKit runtime workaround

Physically reproduced on Fedora 39 with nouveau: the earlier Ubuntu 24.04-built
1.0.0 release candidate started PULSE, then `WebKitWebProcess` crashed in
`nouveau_pushbuf_data` with the `kref` assertion. That AppImage worked with only
`WEBKIT_DISABLE_DMABUF_RENDERER=1`; `LIBGL_ALWAYS_SOFTWARE`, llvmpipe and other
driver overrides were unnecessary.

Before WebKit starts, PULSE therefore checks exact DRM `card<N>` entries in
`/sys/class/drm`. It sets `WEBKIT_DISABLE_DMABUF_RENDERER=1` only when a card's
`device/driver` symlink ends in `nouveau`, and only when the user has not
already defined that variable. Connector, render and control nodes are ignored;
other drivers and existing user choices are unchanged.

## Testing note

Normal CI runs on `ubuntu-latest`; Release compilation and packaging are pinned
to Ubuntu 22.04 for their GLIBC 2.35 baseline. That catches Linux regressions —
wrong `cfg` gating, missing Linux paths, build breakage — but **it is not a
Fedora test**. Different kernel, different WebKitGTK, different GNOME, different
sensor modules. Real Fedora validation is manual and is required before any
system-facing feature is considered complete.
