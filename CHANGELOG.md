# Changelog

All notable changes to PULSE are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added — Phase 6: Storage inventory, I/O, volumes & NVMe health

Physical storage devices, mounted filesystems, real I/O rates and the
standardised NVMe health log, on both platforms, built around one distinction:
**a device is not a volume, and a volume is not a mount point.**

- **Two machine-wide metrics** — `storage.device.count` and
  `storage.volume.count` (`storage:system`, count, state).
- **Thirteen metrics per physical device** — `storage.capacity.total` (bytes);
  `storage.io.read`/`write.bytes_per_second`, `.iops` and `.latency`; and the
  six `storage.health.*` values: temperature, used endurance, available spare,
  power-on hours, unsafe shutdowns and media errors.
- **Four metrics per volume** — `storage.volume.capacity.total`, `.used`,
  `.available` and `storage.volume.usage.percent`.
- **A catalog still sized by the machine** — `2 + 13D + 4V` storage metrics for
  `D` devices and `V` volumes, bringing the total to
  `11 + 3N + P + 11G + 13D + 4V`: **169** on the reference machine (32 threads,
  1 package, 1 GPU, 2 disks, 6 filesystems). Nothing hardcodes any of them, and
  the tests derive the expected size from the catalog.
- **Provider count goes to 4.** `linux.storage` and `windows.storage` each own
  five backends — inventory, volumes, filesystem usage, I/O counters and NVMe
  health. There is deliberately no `linux.nvme`, `storage.smart` or `filesystem`
  provider: a disk's inventory and its health describe the same device, so two
  providers would claim the same `SourceId` and the engine would reject one.

Two small, deliberate additions to the shared contract, the first this project
has needed since Phase 1:

- **Units `operationsPerSecond` and `hours`.** A *rate* of operations is not a
  `count` of them — only one depends on the interval it was measured over — and
  an NVMe controller counts power-on time in whole hours, so expressing it in
  `seconds` would invent five orders of magnitude of precision. A contract test
  asserts no `storage.io.*` metric carries a quantity unit.
- **Source kind `volume:`.** A disk holds many filesystems, a filesystem can
  span disks, and one filesystem is often reachable at several paths at once.
  `storage:` names hardware and `volume:` names a filesystem, so a widget bound
  to one can never resolve to the other.

Where the numbers come from:

| Layer            | Fedora                                           | Windows                                                        |
| ---------------- | ------------------------------------------------ | -------------------------------------------------------------- |
| Inventory        | `/sys/class/block`                               | SetupAPI `GUID_DEVINTERFACE_DISK` + `IOCTL_STORAGE_QUERY_PROPERTY` |
| Capacity         | `/sys/block/<dev>/size`                          | `IOCTL_DISK_GET_DRIVE_GEOMETRY_EX`                              |
| I/O counters     | `/proc/diskstats`, one read for every device     | `IOCTL_DISK_PERFORMANCE`, per device by handle                  |
| Volumes          | `/proc/self/mountinfo`                           | `FindFirstVolumeW` + `GetVolumePathNamesForVolumeNameW`         |
| Volume usage     | `statvfs(3)`                                     | `GetDiskFreeSpaceExW`                                           |
| Volume → device  | the mount's device number, via `/proc/diskstats` | `IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS`                          |
| NVMe health      | `nvme` `hwmon` + `NVME_IOCTL_ADMIN_CMD`          | `IOCTL_STORAGE_QUERY_PROPERTY` + `ProtocolTypeNvme` log `0x02`  |

What it deliberately refuses to publish, each enforced by a test:

- **`0 B/s` or `0 IOPS` before a baseline exists.** Activity is a rate: the
  first sample has nothing to difference against and says so, rather than
  reporting an unmeasured disk as idle.
- **`0 ms` as a latency when no operation completed.** A latency is a mean over
  completed operations; an interval with none has no mean. `0 B/s` and `0 IOPS`
  over a real interval *are* published, because they are measurements.
- **A spike after a counter reset.** A suspend/resume, a reconnected USB disk or
  Windows's 32-bit operation counters wrapping make a total go down; any single
  field regressing restarts the baseline rather than publishing a delta of four
  billion.
- **`size × logical_block_size` as a capacity, or as a throughput.** Kernel
  block statistics count fixed 512-byte sectors whatever the device's logical
  block size; the other formula reports **eight times** the real figure on a 4Kn
  drive. Explicit tests pin the correct constant.
- **`used = total - available`.** Unix filesystems reserve blocks for the
  superuser, so `used` is `total - free` and `available` is what this user can
  actually write — matching `df` byte for byte. The other formula shows several
  gigabytes of phantom usage on every ext4 volume.
- **A clamped `percentage_used`, or `100 - percentage_used` as a health score.**
  The NVMe specification permits values above 100 once a drive passes its rated
  endurance, which is exactly the reading a user must see. PULSE publishes no
  verdict, no score and no grade — a test asserts no storage key contains
  `score` or `status`.
- **`available_spare` presented as free space.** It is the controller's reserve
  of replacement blocks; a completely full drive normally still reports 100 %.
- **A per-die NVMe sensor as the device temperature.** The composite channel is
  found by **label**, never by taking `temp1_input` positionally — on the
  reference machine the per-die sensors read 52.85 °C and 57.85 °C against a
  composite of 52.85 °C.
- **A guessed parent for a volume.** Attribution comes from the mount's device
  number or the volume's disk extents, never from a name; a volume that cannot
  be correlated is shown separately rather than attached to the wrong disk.
- **Partitions, loop devices, `zram` and device-mapper volumes as disks.** One
  NVMe drive with eight partitions counts as one device, and the eight `loop`
  devices Fedora creates for snaps count as none. A `virtio` disk *is* counted:
  the filter is about whether an entry is a usable block device, not about
  whether silicon is involved.
- **One filesystem as several volumes.** Fedora mounts the same btrfs filesystem
  at `/` and `/home`; a bind mount adds a third path. They are one volume with
  three mount points, not three volumes with the machine's capacity counted
  three times.

Also in this phase:

- **Read-only, without exception.** Nothing mounts, unmounts, partitions,
  formats, trims or repairs. The only NVMe command PULSE can express is
  `Get Log Page 0x02` — the opcode is a constant, never a parameter — and every
  Windows device handle is opened with `dwDesiredAccess = 0`, which grants
  neither read nor write access to a single sector.
- **No subprocesses.** No `lsblk`, `blkid`, `df`, `udevadm`, `smartctl`, `nvme`,
  `PowerShell`, `wmic`, `diskpart`, `fsutil`, `Get-PhysicalDisk` or `Get-Disk`,
  anywhere.
- **PULSE still does not need root.** `/dev/nvme0` is root-only on Fedora, so an
  unprivileged run gets the inventory, the volumes, the I/O counters and the
  composite temperature — and reports the other five health values as
  `permissionDenied`, never as `unsupported`, because "the OS refused" and "your
  drive has no health data" are different statements.
- **ATA SMART is deferred, not refused.** Its attributes are vendor-defined, and
  normalising them wrongly would publish confident but false claims about a
  user's disk. A SATA or USB device keeps all six `storage.health.*` definitions
  in the catalog with a reason that says PULSE has no backend yet.
- **Identity is recorded, not assumed.** `nvme0n1`, `sda`, `PhysicalDrive0`,
  `C:`, `/` and `major:minor` are never identities. PULSE prefers a WWN, NGUID,
  EUI-64 or T10 identifier, then a serial, then an OS-assigned stable id, and
  carries an `IdentityStability` saying which it used — so a session-scoped
  fallback is inspectable rather than silently fragile. A drive's serial is the
  same string on both platforms, so **the same disk gets the same `SourceId`**
  on Fedora and Windows, and a contract test asserts it.
- **USB placeholder serials are refused.** Many bridges ship every unit with
  `0123456789ABCDEF`; two disks in two identical enclosures would otherwise
  collapse onto one identity.
- **A USB disk is named by its enclosure** when the bridge answers the SCSI
  inquiry with a protocol name — the reference machine's external disk reads
  `Intenso USB3.0 Device` rather than `Intenso SCSI`.
- **Windows parsing is tested on Fedora.** The `STORAGE_DEVICE_DESCRIPTOR`
  string offsets, the bus-type mapping, the NVMe log-page extraction, the
  `DISK_PERFORMANCE` 100 ns → ms conversion, the disk-extent parsing and the
  identity rules are all pure functions over byte slices, unit-tested against
  synthetic buffers with no Windows machine involved.
- **UI** — a new *Storage details* card: a compact grid of devices, each with its
  capacity, six activity rows and six health rows, and its volumes nested
  underneath with a usage bar. A volume PULSE could not attribute appears under
  *Other volumes*. Before a baseline exists the card says *Waiting for another
  sample* rather than showing zeros, and it does **not** fire a hidden second
  request to paper over it. Still one sample on mount and one per click of
  Refresh: no polling, no scheduler, no subscription.
- **Documentation** — [`docs/metrics/storage.md`](docs/metrics/storage.md).

### Added — Phase 5: Thermals & Cooling

Temperatures and fan speeds on both platforms, built around a single rule: PULSE
publishes a reading it can name a source for, and explains every one it cannot.

- **`cpu.temperature.package`** (`cpu:package-N`, celsius, gauge) — the
  processor package's own sensor. On Fedora through `hwmon`: `coretemp`'s
  `Package id N`, `k10temp`'s `Tdie`, or `peci_cputemp`'s `Die`. **Unsupported on
  Windows**, which offers no interface for it that does not require a
  kernel-mode driver — the metric is still declared, with the reason, so a
  dashboard built on Fedora resolves and explains itself there.
- **Four metrics per GPU** — `gpu.temperature.core`, `gpu.temperature.hotspot`,
  `gpu.temperature.memory` (celsius) and `gpu.fan.speed` (**rpm**), from NVML or
  from the card's own `hwmon` node (`amdgpu` `edge`/`junction`/`mem`, a
  `nouveau` single channel, `fanN_input`).
- **A catalog still sized by the machine** — `9 + 3N + P + 11G`, where `P` is the
  number of packages PULSE can address as a measurement source. 117 metrics on
  the 32-thread, single-package, single-GPU reference machine. Nothing hardcodes
  `N`, `P` or `G`.
- **New sources `cpu:package-N`**, numbered with the kernel's own
  `physical_package_id` — the same numbering `cpu.count.package` is counted from,
  never a second thermal-only one.
- **Provider count unchanged at 3.** `hwmon` is a capability of the CPU and GPU
  providers, not a provider of its own: a `linux.hwmon` provider would claim the
  same sources they already own, and the engine would reject one of them.

What it deliberately refuses to publish, each enforced by a test:

- a **thermal limit** as a temperature — `Tjmax`, `Tcontrol`, `Tthrottle` and
  `tempN_crit` are never read as measurements (this is what makes other tools
  report an idle laptop at 100 °C);
- an **average of per-core sensors** as a package temperature — it reads lower
  than the truth exactly when a single boosting core is throttling the machine;
- AMD's **`Tctl`** as a die temperature — it carries a deliberate offset on many
  parts, so a CPU exposing only `Tctl` publishes nothing;
- a **GPU die temperature** as a hotspot or a memory temperature — three
  distinct sensors, never derived from one another;
- a **fan control percentage** as an RPM — a duty cycle is not a speed, and a
  multi-fan board publishes no single speed rather than picking one;
- **`0` for anything absent** — while a genuine `0 RPM` from a stopped fan is
  published as the reading it is.

Also in this phase:

- **Read-only, without exception.** No `pwm`, no fan curve, no temperature or
  power limit, no overclock or undervolt is ever written — and the control files
  are never opened at all.
- **`hwmonN` is never an identity.** Sensors are found by driver name, by the
  hardware their `device` symlink resolves to, and by channel label, so a probe
  order change or a driver reload cannot silently re-point a metric.
- **Millidegrees are converted once**, at the platform edge. The contract carries
  Celsius; nothing above the platform layer knows another unit exists.
- **NVML thermal symbols are optional**, resolved at runtime:
  `nvmlDeviceGetTemperatureV` preferred, `nvmlDeviceGetTemperature` as a
  fallback on any failure of it, and `nvmlDeviceGetFanSpeedRPM` alongside
  `nvmlDeviceGetNumFans`. A library exporting none of them costs one metric and
  leaves every Phase 4 figure working.
- **UI** — the CPU card shows a package temperature row per package, and each GPU
  gains temperature, hotspot, memory temperature and fan rows. Still one sample
  on mount and one per click of Refresh: no polling, no scheduler, no
  subscription.
- **Documentation** — [`docs/metrics/thermals.md`](docs/metrics/thermals.md).

### Fixed — monitoring UX and Windows GPU discovery

- **CPU Details shows every processor.** The card used to scroll inside itself
  above roughly 28 rows, which on a 32-thread machine hid exactly four
  processors behind a second scroll context inside a page that already scrolls.
  The card now grows to fit the machine; above 64 logical processors the list is
  collapsed behind an explicit *Show all N processors* control rather than
  clipped. The GPU list lost its inner scrollbar for the same reason.
- **An adapter with no performance telemetry says so, in the card.** A GPU whose
  driver exposes no utilisation, VRAM or clock figures showed five dashes and
  put the explanation in tooltips nobody has a reason to open. It now carries a
  visible *Performance telemetry unavailable* notice with the backend's own
  reason. The wording is deliberately not "GPU unavailable": the adapter is
  detected, named and identified, and its thermal sensors are tracked
  separately, so a card with working temperatures is never described as having
  no telemetry.
- **NVML is found where NVIDIA actually installs it.** In addition to
  `%SystemRoot%\System32\nvml.dll`, PULSE now looks for
  `<Program Files>\NVIDIA Corporation\NVSMI\nvml.dll`, loaded by absolute path
  with its dependency search confined to System32 and its own directory. Program
  Files is located with `SHGetKnownFolderPath(FOLDERID_ProgramFiles)` rather than
  read from `%ProgramW6432%`, which is inherited and therefore attacker-settable.
  PULSE still never falls back to the default DLL search path.
- **Windows GPUs are correlated by PCI bus address, not by enumeration order.**
  `D3DKMTOpenAdapterFromLuid` + `D3DKMTQueryAdapterInfo(KMTQAITYPE_ADAPTERADDRESS)`
  now resolves a DXGI adapter's bus address, which is matched against NVML's.
  Where no address is available, an adapter and a device are paired **only** when
  exactly one of each is unmatched; two NVIDIA cards are never paired by order,
  because the two APIs enumerate independently and a wrong pairing writes the
  wrong card's identity into a saved dashboard. Windows identity is unchanged —
  the address is correlation data, so no stored `SourceId` moves.
- **A Windows cross-check harness** (`tools/windows-check/`) type checks the
  `metrics/` and `platform/` trees for `x86_64-pc-windows-msvc` from a Fedora
  workstation, where the full Tauri build cannot run for want of a Windows
  resource compiler. It includes the application's own files by path and pins the
  same `rust-version`.

### Added — Phase 4: GPU Inventory & Core Metrics

PULSE's first real GPU support, on both Fedora Linux and Windows, across NVIDIA,
AMD and Intel hardware — and honest about what each of them does not expose.

- **`gpu.count`** (`gpu:system`, count, **state**) — hardware adapters
  inventoried. Software renderers such as Microsoft Basic Render Driver and WARP
  are deliberately not counted as GPUs.
- **Seven metrics per GPU** — `gpu.usage.core` (percent), `gpu.memory.total`
  (bytes, **state**, because installed VRAM is a hardware fact rather than an
  averageable reading), `gpu.memory.used`, `gpu.memory.free`,
  `gpu.memory.usage.percent`, `gpu.frequency.core` and `gpu.frequency.memory`
  (hertz).
- **A catalog sized by the machine** — `1 + 7G` GPU metrics added to the CPU and
  memory families, for a total of `9 + 3N + 7G`. Nothing hardcodes `G`, in Rust
  or in React. A headless machine publishes `gpu.count` reporting zero, which is
  a fact rather than a failure.
- **Stable GPU identity, and none of the tempting wrong answers.** A GPU is
  never identified by its product name, its DRM card number, its NVML index or
  its DXGI adapter index — all of which are enumeration artefacts that change
  between boots. PULSE uses, in descending order of strength: an **NVML hardware
  UUID**, a **PCI bus address**, or a **device-model tuple** with a
  session-scoped disambiguator when two adapters share it. The stability level
  is recorded in the descriptor rather than assumed, so a weaker guarantee is
  inspectable instead of silent.
- **One GPU provider per platform**, hosting several vendor backends —
  `linux.gpu` over DRM, NVML and `amdgpu`; `windows.gpu` over DXGI and NVML.
  Registering `nvidia.nvml` separately would make two providers claim the same
  `MetricRef` for a card both can see, which the engine rejects by design.
  Provider count is now **3** per platform.
- **Deduplication** — a card seen by both the generic inventory and a vendor
  backend is published once, under the stronger identity. Matched by PCI address
  where both report one (Fedora), and by vendor and enumeration order where DXGI
  exposes none (Windows). Never by product name.
- **NVML loaded at runtime on both platforms**, extending the rule Phase 3
  established: an optional monitoring backend is not a mandatory application
  dependency. `dlopen("libnvidia-ml.so.1")` on Fedora — the SONAME, not the
  CUDA development symlink — and
  `LoadLibraryExW(L"nvml.dll", NULL, LOAD_LIBRARY_SEARCH_SYSTEM32)` on Windows.
  The Windows flag is a security decision: a plain `LoadLibraryW` searches the
  application directory first, so anyone able to drop a file beside `pulse.exe`
  could have PULSE load their DLL. PULSE does not widen the search on failure.
- **Granular NVML degradation** — `NOT_SUPPORTED`, `NO_PERMISSION`,
  `GPU_IS_LOST`, `NOT_FOUND` and `UNINITIALIZED` map to four different
  availabilities plus a provider error, never all to "provider error". A card
  whose memory clock is unsupported keeps its usage, VRAM and core clock.
- **AMD telemetry from `amdgpu` sysfs** — `gpu_busy_percent`,
  `mem_info_vram_{total,used}`, and the active clock state from `pp_dpm_sclk`
  and `pp_dpm_mclk`, whose `*` marker, spacing and `Mhz` capitalisation vary by
  driver version. An absent file costs that metric, never the GPU.
- **Fedora inventory from `/sys/class/drm`** — only `card<N>` entries resolving
  to a real PCI device count, so display connectors, render nodes and virtual
  devices such as `vkms` are excluded. Device names come from the system PCI ID
  database read as an ordinary file — the same data `lspci` prints, without
  running it.
- **Windows inventory from DXGI**, with `QueryVideoMemoryInfo` deliberately
  **not** published as VRAM usage: its `CurrentUsage` is the querying process's
  own consumption, so it would read near zero while a game filled the card.
  `DedicatedVideoMemory` is published as the installed capacity, and the live
  figures are reported `unsupported`.
- **VRAM means dedicated video memory.** System memory shared with an integrated
  GPU is never turned into pretend VRAM. Memory arithmetic refuses a zero total,
  `used` above `total`, `free` above `total`, and an overflowing `used + free`,
  so nothing published is ever `NaN`, infinite, negative or above 100.
- **GPU details card** on Overview — adapters discovered from the catalog, laid
  out in an auto-filling grid that grows in columns rather than height, with its
  own Refresh. An unmeasured metric shows `—` with the reason in its tooltip,
  never `0 %`, `0 GiB` or `0 GHz`. Identical cards are numbered `#1`, `#2` for
  display only, leaving their identity untouched.
- **Still no scheduler** — one sample on mount, one per Refresh, and a test that
  advances timers and asserts no further request is made. Identity, names and
  capabilities are discovered once; only the values that move are re-read.

### Changed

- `wellknown::units` now holds the shared hertz conversions, which GPU clocks
  need as much as CPU ones; `cpu::frequency` re-exports them unchanged.
- The engine-status and state tests no longer assume two providers or a CPU-only
  catalog; both counts are derived from the catalog itself.
- `windows` (COM bindings) added as a direct dependency for DXGI, gated to
  Windows. It is already in the graph there via Tauri, so it adds no build
  weight.
- Fixed a latent MSRV violation: `Option::is_none_or` is newer than the declared
  `rust-version` of 1.77.2.

### Documentation

- New [`docs/metrics/gpu.md`](docs/metrics/gpu.md) — the four problems GPU
  support solves, identity and why every obvious candidate is wrong, the
  provider architecture and why backends are not separate providers,
  deduplication, NVML loading and its security reasoning, per-vendor
  degradation, DRM discovery, AMDGPU sysfs, Windows inventory, VRAM semantics,
  multi-GPU, and per-refresh cost.
- Updated `docs/metrics/README.md`, `docs/metrics/providers.md`,
  `docs/metrics/identifiers.md`, `docs/platforms/fedora.md`,
  `docs/platforms/windows.md`, `docs/architecture/overview.md` and `README.md`.

## [Phase 3]

### Fixed

- **`NtQuerySystemInformationEx` is now resolved dynamically** rather than
  imported at load time. It backs `cpu.usage.logical` on Windows and lives in
  `ntdll`, which Microsoft documents as subject to change — so an `extern`
  block made it a load-time import, and on a Windows that does not export the
  symbol the loader would have failed the process **before PULSE could report
  anything**. That inverted the principle the availability model exists to
  enforce: a capability being unavailable is not the application being unable
  to start.

  The symbol is now looked up once at provider construction with
  `GetModuleHandleW("ntdll.dll")` and `GetProcAddress`, then probed with one
  undersized query to confirm it understands the information class.
  `GetModuleHandleW` rather than `LoadLibrary`: `ntdll` is already mapped into
  every Win32 process, so nothing touches the filesystem and no arbitrary
  library is loaded. There is no `#[link]` attribute and no `extern "system" { }`
  block left anywhere in PULSE.

  If the module, the symbol, or the probe fails, the result is a structured
  `Unsupported` availability — no `unwrap`, no `expect`, no panic, and
  `WindowsCpuProvider` still constructs. Only `cpu.usage.logical` becomes
  unavailable; `cpu.usage.total` (`GetSystemTimes`), `cpu.count.*`
  (`GetLogicalProcessorInformationEx`), `cpu.frequency.*`
  (`CallNtPowerInformation`) and `memory.*` all keep working, because each sits
  on its own entry point. This is also why the aggregate is read from
  `GetSystemTimes` rather than summed from the per-processor array.

  The per-processor metrics **stay in the catalog** with an honest status
  instead of disappearing, so a dashboard holding
  `cpu.usage.logical@cpu:logical-3` keeps the same reference on a machine
  without the capability. Provider count is unaffected and stays at 2.

  Resolution happens once and the handle is reused: no `GetProcAddress` per
  refresh or per processor, and no mutable global. The raw pointer is private
  to a safe wrapper and never circulates through the provider.

- Per-processor counter reads now go through a `ProcessorTimesSource` seam, so
  the group stitching, short-reply handling and every failure path are unit
  tested from Fedora against a fake source rather than requiring the real DLL.
- `records_in_reply` replaced `usize::is_multiple_of`, which is newer than the
  crate's declared `rust-version` of 1.77.2 — a latent MSRV violation that was
  invisible while the code sat behind `cfg(target_os = "windows")`.

### Added — Phase 3: Advanced CPU Metrics

Real per-processor CPU detail on both Fedora Linux and Windows, behind one
contract. PULSE can now say how many physical cores and logical processors a
machine has, what each logical processor is doing, and at what frequency the OS
reports it running.

- **`cpu.usage.logical`** (`cpu:logical-N`, percent) — per logical processor.
  `/proc/stat`'s `cpuN` lines on Fedora; `NtQuerySystemInformationEx` with
  `SystemProcessorPerformanceInformation` on Windows. The formula is identical
  to `cpu.usage.total`'s, so the two reconcile instead of being two definitions
  of "busy" that share a name.
- **`cpu.frequency.current`** and **`cpu.frequency.max`** (`cpu:logical-N`,
  hertz) — `cpufreq/scaling_cur_freq` and `cpufreq/cpuinfo_max_freq` on Fedora;
  `CallNtPowerInformation(ProcessorInformation)` on Windows.
- **`cpu.count.logical`, `cpu.count.physical`, `cpu.count.package`**
  (`cpu:system`, count) — `/sys/devices/system/cpu/` topology on Fedora;
  `GetLogicalProcessorInformationEx` on Windows. Declared as `state` rather than
  `gauge`, because averaging a core count over time is meaningless and
  `MetricKind::State` refuses it by construction.
- **A catalog sized by the machine** — `8 + 3N` metrics for `N` logical
  processors: 104 on a 32-thread laptop, 20 on a four-thread virtual machine.
  No count is hardcoded anywhere, in Rust or in React; `wellknown::cpu` exposes
  a generator over a discovered topology rather than a constant list. The
  provider count stays at **2** per platform — one CPU provider owns every CPU
  metric on the machine, not one provider per processor.
- **Logical processor, physical core and package kept rigorously distinct.**
  With simultaneous multithreading two logical processors share one physical
  core; on a hybrid CPU the ratio is not even constant (8 P-cores × 2 threads
  plus 16 E-cores = 24 cores, 32 logical processors). Physical cores are counted
  as distinct `(package_id, core_id)` pairs on Linux and as
  `RelationProcessorCore` **records** on Windows — never as set affinity bits,
  which would count threads.
- **`cpu:logical-N` identifiers**, documented as *logical slots of this system*
  rather than hardware serial numbers: stable across reboots and restarts on one
  machine, meaningless on another. On Linux the ordinal is the kernel's own CPU
  number, so it matches `htop` and `taskset`.
- **Windows processor groups handled properly** — the implementation is not
  capped at 64 logical processors. PULSE assigns ordinals by sorting
  `(group, index in group)`, so `group 1 bit 0` becomes `cpu:logical-64` and
  never collides with `cpu:logical-0`. The mapping is deterministic, so it
  cannot shift between runs and silently re-point saved references. Tests cover
  128- and 256-processor layouts across two and four groups, from Fedora.
- **Hertz everywhere on the wire.** Linux CPUFreq reports kHz and the Windows
  power API reports MHz; both are converted in the platform layer, with overflow
  checks. A zero reading means "not reported" on both platforms and is published
  as unavailable — never as `0 Hz`, which renders as `0 GHz` and reads as a
  claim that the core has stopped.
- **A multi-processor CPU baseline** behind a single mutex holding the aggregate
  and every logical processor. A processor that appears, disappears, reports
  counters that rewind, or reports no elapsed time is answered with
  `temporarilyUnavailable` and a reason — never `0%` — and its baseline is
  re-primed so the next request succeeds. A departed processor's stale baseline
  is dropped, so one that comes back does not difference against pre-offline
  counters.
- **Failure granularity per metric, per processor.** A missing
  `cpu17/cpufreq` costs `cpu17`'s frequency and nothing else; `cpu.usage.total`,
  every `cpu.usage.logical` and the topology counts keep working. On Windows the
  aggregate deliberately stays on the documented `GetSystemTimes`, so it
  survives the per-processor counter call being unavailable. A count the
  platform genuinely cannot determine is `notDetected` with a reason, never
  guessed by halving the logical count.
- **CPU details card** on Overview — physical cores, logical processors and
  packages, then a compact auto-filling grid of every logical processor with its
  usage, a meter and its current frequency, with the maximum in the tooltip. It
  has its own Refresh button and stays usable from 4 to 128+ processors. An
  unknown frequency shows `—` with the reason in its tooltip, never `0 GHz`.
- **The frontend discovers processors from the catalog** rather than holding any
  list of CPUs, and **sorts them numerically**: the backend's `(key, sourceId)`
  string ordering yields `logical-1, logical-10, logical-11, logical-2`, which
  would scramble the table. A processor appears as soon as any one of its
  metrics does, so a missing frequency never costs it a row.
- **`formatHertz`** (`src/utils/units.ts`) — Hz to `800 MHz` / `3.20 GHz` at the
  display edge only. The contract stays in hertz; no GHz exists in Rust.
- **Still no scheduler.** One sample on mount, one per Refresh click, and a test
  that advances timers by sixty seconds and asserts no further request is made.

### Changed

- `MetricsEngineCard` and the engine-status tests no longer assume five metrics;
  the count is read from the engine and is now machine-dependent.
- `CpuUsageTracker` takes a whole-machine `CpuSnapshot` rather than a single
  pair of counters, and reports self-contradictory counters as a transient
  unavailability with a re-primed baseline rather than as a hard error — the
  same treatment as every other unmeasurable case.
- `metrics::wellknown::cpu` became a module directory (`topology`, `usage`,
  `frequency`), and `cpu::definitions` now takes the discovered topology.
- `windows-sys` gained the `Win32_System_Power`,
  `Win32_System_WindowsProgramming` and `Wdk_System_SystemInformation` features.

### Documentation

- New [`docs/metrics/cpu-advanced.md`](docs/metrics/cpu-advanced.md) — the
  logical/physical/package vocabulary, the dynamic catalog, `cpu:logical-N`
  stability and its implication for future dashboard selectors, both platforms'
  data sources, the Windows per-processor API comparison and why
  `NtQuerySystemInformationEx` was chosen, processor groups, hybrid CPUs, the
  hertz contract, what `cpu.frequency.current` does and does not claim, the
  tracker, and the per-refresh cost on each platform.
- Updated `docs/metrics/README.md`, `docs/metrics/providers.md`,
  `docs/metrics/identifiers.md`, `docs/platforms/fedora.md`,
  `docs/platforms/windows.md`, `docs/architecture/overview.md` and `README.md`.

## [Phase 2]

### Added — Phase 2: Real CPU & Memory Metrics

PULSE's first real system metrics, implemented natively on both Fedora Linux
and Windows. Five metrics, sharing one set of references across both operating
systems.

- **`cpu.usage.total`** (`cpu:system`, percent) — `/proc/stat` on Fedora,
  `GetSystemTimes` on Windows.
- **`memory.total`, `memory.used`, `memory.available`** (`memory:system`,
  bytes) and **`memory.usage.percent`** — `/proc/meminfo` on Fedora,
  `GlobalMemoryStatusEx` on Windows.
- **Providers** `linux.cpu` / `linux.memory` and `windows.cpu` /
  `windows.memory`, registered automatically at startup by the platform layer.
- **One PULSE memory convention on both platforms** — `used = total -
  available`, `usage_percent = used / total * 100`, so `used + available` always
  equals `total` exactly. `MemAvailable` is used rather than `MemFree`, which
  would make a machine with a warm page cache look nearly full. Windows'
  rounded `dwMemoryLoad` is ignored in favour of computing from the byte counts,
  so the percentage agrees with the figures shown beside it.
- **Shared metric declarations** in `metrics/wellknown/` — key, source, unit,
  kind and user-facing text live in one place, and platform providers supply
  only raw counters. A contract test asserts the Linux and Windows declarations
  differ in `providerId` and nothing else, which is what lets a dashboard move
  between operating systems.
- **CPU baseline without blocking** — usage is a rate, so each provider captures
  a baseline at construction and each request compares against the previous
  reading. No `sleep` inside a command. When no usable delta exists the sample
  is `temporarilyUnavailable` with a reason, never `0%`, and the baseline is
  reset so the next request succeeds.
- **`HostPlatform::metric_providers()`** — the platform layer decides which
  providers exist; `services::metrics::build_engine()` composes them. The engine
  still contains no `cfg(target_os)` at all.
- **Live system sample card** on Overview, with a Refresh button and the sample
  timestamp. It shows real values only; an unavailable metric is explained
  rather than shown as zero.
- **Display formatting helpers** (`src/utils/units.ts`) — bytes to GiB/MiB,
  percentages, sample times. Presentation only: the contract still carries
  bytes, percent and Unix epoch milliseconds.
- **Tests** — 216 Rust (was 122) and 44 frontend (was 22), including
  `/proc/stat` and `/proc/meminfo` fixtures for malformed, truncated and
  unusual input, the Windows counter arithmetic, and host tests that assert
  invariants rather than a particular amount of RAM.
- **Documentation** — new `docs/metrics/cpu-memory.md`; metrics README,
  providers, identifiers, both platform guides, architecture overview and README
  updated.

### Changed

- Both platform modules now compile on **every** host, with only the FFI calls
  and `HostPlatform` implementations gated behind `cfg(target_os)`. A Fedora
  test run therefore exercises the Windows arithmetic and vice versa — there was
  no good reason for Windows maths to be untestable from a Linux machine.
- `metrics::build_engine` takes providers as an argument instead of building an
  empty engine, keeping the metrics layer free of platform knowledge.

### Notes

- **No scheduler and no polling.** The model is still
  `request → sample → response`; the UI refreshes on demand. A hidden interval
  in the frontend would be a scheduler in disguise, and a test asserts the card
  does not poll.
- **No elevated privileges.** All five metrics work as an ordinary user on both
  platforms.
- `guest` and `guest_nice` are excluded from the `/proc/stat` total — the kernel
  already counts them inside `user` and `nice`, and adding them again is the
  classic bug that makes a busy host look idle.
- Windows counts idle time inside `KernelTime`; applying the Linux formula there
  would report an idle machine as heavily busy.
- Dependencies: only `windows-sys` (raw FFI, Windows-only) was added. No
  cross-platform monitoring crate — validating PULSE's own native architecture
  is part of what this phase is for.

### Added — Phase 1: Metrics Engine Foundation

The universal metrics contract. **No hardware data is collected yet**: the
engine registers no providers, so PULSE reports an empty catalog rather than
inventing numbers.

- **Metric model** (`src-tauri/src/metrics/model/`) — validated `MetricKey`,
  `SourceId`, `ProviderId` and `MetricRef`; `MetricDefinition`, `MetricSample`,
  `MetricValue`, `MetricUnit`, `MetricKind`, `MetricCategory`, `Availability`
  and `MetricError`.
- **Identity separated from presentation** — `MetricKey` says *what* is
  measured, `SourceId` says *on what*. A device's product name is never an
  identifier, so two identical drives stay distinguishable and a renamed device
  does not break saved dashboards.
- **Canonical units** — the backend always reports hertz, bytes and Celsius;
  display conversion belongs to the frontend.
- **Seven availability states** — `unsupported`, `notDetected`,
  `permissionDenied`, `temporarilyUnavailable`, `providerError` and
  `notRegistered` stay distinguishable from `available` and from each other.
- **`MetricProvider` trait** — synchronous, `Send + Sync`, with structured
  errors.
- **`MetricsEngine`** — provider registration with atomic collision detection,
  a deterministically ordered catalog, `HashMap`-indexed reference resolution,
  request-order sampling with per-provider deduplication, and failure isolation
  so one broken provider cannot blank out the others.
- **`METRICS_SCHEMA_VERSION = 1`** — an explicit contract version, mirrored in
  TypeScript and checked by the UI.
- **Tauri commands** — `get_metrics_engine_status`, `get_metric_catalog`,
  `sample_metrics`, backed by an `Arc<MetricsEngine>` in Tauri state.
- **TypeScript contract** — `src/types/metrics.ts` and `src/services/metrics.ts`,
  with no `any`.
- **Metrics Engine card** on Overview — an architecture check reporting status,
  schema version and counts. It displays no hardware readings.
- **Tests** — 122 Rust tests (up from 14) and 22 frontend tests (up from 9),
  including contract tests that pin every payload's exact field set so Rust and
  TypeScript cannot drift apart silently.
- **Documentation** — `docs/metrics/model.md`, `docs/metrics/identifiers.md`,
  `docs/metrics/providers.md`; `docs/metrics/README.md` and
  `docs/architecture/overview.md` updated.

### Notes

- A non-finite float is rejected at construction: `serde_json` would serialise
  `NaN` as JSON `null` while the sample still claimed to be available.
- Provider *panics* are not sandboxed. `panic = "abort"` in the release profile
  makes `catch_unwind` a debug-only guarantee, so the contract requires
  providers not to panic instead of pretending to contain them. Revisiting this
  is a deliberate open decision recorded in `docs/metrics/providers.md`.

## [0.1.0-dev] — Phase 0: Foundation

The foundation of PULSE. No monitoring functionality yet — this release
establishes the architecture everything else will be built on.

### Added

- **Project foundation** — Tauri 2 + React 19 + TypeScript + Vite + Rust,
  managed with pnpm.
- **Platform abstraction layer** (`src-tauri/src/platform/`) — a `HostPlatform`
  trait with Linux and Windows implementations, plus an `UnsupportedPlatform`
  fallback. This is the only place in the codebase that branches on the
  operating system.
- **Per-target dependencies** — Windows-only crates are declared under
  `[target.'cfg(target_os = "windows")'.dependencies]`, so a platform dependency
  can never break the other OS.
- **`get_platform_info` command** — the first real React → Tauri → Rust →
  React round trip, returning OS, architecture, OS version, display server
  (Linux) and the PULSE version.
- **Application shell** — a dark-themed interface with client-side navigation
  across Overview, Gaming, Development, Personal and Mini.
- **Status bar** — reports the platform detected by the backend, and degrades
  gracefully when PULSE runs outside the Tauri runtime.
- **Linux platform support** — `/etc/os-release` parsing and Wayland/X11
  display-server detection.
- **Windows platform support** — OS version reporting, distinguishing
  Windows 10 from Windows 11 by build number.
- **Tests** — 14 Rust unit tests and 9 frontend tests covering platform
  detection, payload serialisation, parsing logic, navigation and graceful
  degradation.
- **CI** — GitHub Actions on Ubuntu and Windows: install, lint, typecheck,
  tests and builds for both the frontend and the Rust backend.
- **Documentation** — architecture overview, Mini overlay design notes, Fedora
  and Windows platform guides, metrics and widget contracts, getting-started
  and testing guides.
- **Repository hygiene** — proprietary licence, contributing guide, security policy,
  issue and pull request templates, EditorConfig, Prettier, ESLint, rustfmt and
  Clippy configuration.

### Notes

- `ubuntu-latest` in CI catches Linux regressions but is **not** a Fedora test.
  Real Fedora validation is manual.
- The `0.1.0-dev` version string is valid SemVer but not valid for MSI
  packaging, which requires a strictly numeric version. This must be resolved
  before the first Windows installer is produced.

[Unreleased]: https://github.com/pulse-monitor/pulse/compare/v0.1.0-dev...HEAD
[0.1.0-dev]: https://github.com/pulse-monitor/pulse/releases/tag/v0.1.0-dev
