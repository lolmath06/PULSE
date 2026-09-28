# Changelog

All notable changes to PULSE are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed — Phase 11 corrective: responsive widgets & overlay lifetime

- **Keep running with visible overlays** now really keeps PULSE running:
  closing the main window only hides it (pure, tested policy
  `main_close_action` / `handle_main_close`), Tauri's implicit exit when the
  last window goes is refused while overlays are visible, and _Open PULSE_
  shows the same main window again (recreated from its configuration only if
  it no longer exists). An explicit Quit still stops everything. Settings are
  written without the save debounce so the backend sees a changed close
  behaviour at once.
- **Responsive density** shared by dashboards, Mini and overlays:
  micro/compact/normal/large from the real box. Micro widgets show their
  primary value (and a sparkline beside it) without stats, axes, grid, legend,
  secondary labels or tooltip; parts get explicit heights and are dropped in
  order, so text is never painted over by a chart and nothing overflows.
  Gauges and narrow bars fall back to a value, bars drop their label first —
  rendering only, the stored configuration is never changed.
- **Compact edit tools** for small widgets: drag handle + “…” menu
  (Customize, Duplicate, To overlay, Remove) + resize grip.
- **Overlay layout** gives every widget an explicit box of its configured size
  (row, column, grid with per-column widths and per-row heights) — no
  `max-content`; _Fit to widgets_ uses the same math; the border is drawn
  inside the box; the Edit bar is laid over the content; the editor preview
  matches the window, scaled uniformly.
- Docs: what the click-through and global-shortcut manual tests mean.

### Added — Phase 11: Modular dashboard, widgets & desktop overlays

- **Shared UI configuration** (`UiConfigStore`): one versioned
  `ui-config.json` in the app config directory for every window, instead of
  per-webview `localStorage`. Atomic writes (temp + fsync + rename, `.bak`),
  corrupt files quarantined and the backup used, newer versions never
  overwritten, a coalescing writer thread, cross-window `ui-config-changed`
  events. Phase 10 visual preferences are migrated once.
- **Dashboards**: several named dashboards (create, rename, duplicate, delete,
  reset with confirmation), a 12-column grid with per-kind limits,
  collision push-down and compaction, repair on load, responsive reflow,
  Edit/Locked, explicit drag handle and resize grip, keyboard move/resize/
  delete/customize, duplicate, import/export JSON without hardware identifiers.
- **Widgets**: four generic kinds (visualization, value, group, summary)
  drawn by the Phase 10 engine; CPU Total bound to `cpu.usage.total`; tiny
  sizes down to 60×20 values and 80×24 sparklines; groups (CPU usage +
  temperature, GPU usage + VRAM + temperature); templates (save, rename,
  delete). *Add widget* lists what the live catalog really offers and explains
  anything disabled. Privacy-safe source references (`get_source_refs`),
  automatic sources labelled as such, *Source unavailable* with remapping.
- **Live widget feed**: one backend 1-second sampler (`LiveHub`) for the union
  of every window's subscription — deduplicated, bounded 300-point rings,
  never persisted, idle when nothing is visible, and refusing NVMe health and
  per-process series. History stays at 5 s in SQLite.
- **Desktop overlays**: separate frameless, transparent, always-on-top Tauri
  windows (`overlay-<id>`), owned and reconciled by the backend. Edit mode
  (drag bar, Lock, Open PULSE, resize grip) and Locked mode (no chrome,
  click-through and unfocusable where the platform allows). Layout (row,
  column, grid), chrome (background, opacity, border, shadow, corners, gap),
  presets (Tiny stats, Thermal strip, Gaming, Minimal corner), send-to-overlay
  from the dashboard and copy back.
- **Overlay capabilities** per display server (Windows, X11, XWayland,
  Wayland), shown in the UI with reasons and refined by runtime facts. Honest
  on GNOME Wayland: always-on-top and click-through *limited*, positioning
  and multi-monitor placement *unsupported*.
- **Monitor-relative logical geometry** with per-monitor scale factors,
  missing-monitor and off-screen recovery.
- **Global shortcut** (default Ctrl+Shift+F12, configurable, conflicts
  reported without losing the previous shortcut), **tray** (Open PULSE, Edit
  overlays, Lock overlays, Show / hide overlays, Quit), and a **close
  behaviour** setting (Quit by default, or keep running while overlays are
  visible, never with nothing on screen).
- **Mini window**: a small ordinary window showing one dashboard.
- Navigation: *Dashboard* and *Overlays*.

### Changed

- Dependencies: `tauri` gains the `tray-icon` feature; new
  `tauri-plugin-global-shortcut` `~2.3` (2.4 requires Rust 1.90). The locked
  graph still builds and tests with Rust 1.77.2.
- New capability file `capabilities/overlay.json` for `overlay-*` windows:
  `core:default` plus window start-dragging and start-resize-dragging only.
  The Mini window uses the default capability.
- `docs/architecture/mini-overlay.md` is marked as Phase 0 notes: the overlay
  it describes is now *overlays*; *Mini* is a small ordinary window.

### Documentation

- New `docs/dashboard/{architecture,widgets,layout}.md` and
  `docs/overlay/{architecture,platform-capabilities,user-guide}.md`; updated
  the architecture overview, Fedora and Windows platform docs (Windows
  protocol · NOT EXECUTED), the MSRV notes and README.

### Added — Phase 10: Persistent history & modular visualization

- **Persistent metric history** (`src-tauri/src/history/`). One backend
  scheduler (`HistoryService`, thread `pulse-history`) samples the historized
  metrics every 5 s and writes **one transaction per batch** to SQLite
  (`rusqlite` 0.32.1, `bundled`, SQLite 3.46.0) in the app's local data directory. WAL,
  `synchronous = NORMAL`, 2 s busy timeout. Schema v1 with structured
  migrations; a newer schema is refused and left untouched.
- **What is historized:** an explicit allow-list of 25 keys across CPU (total,
  per logical processor, package temperature), memory, GPU, storage I/O, network
  traffic and Wi-Fi signal, and the three machine-wide process counts. **Never**
  a per-process series — refused by the selection and again by the store.
- **Unavailable is not zero:** unavailable samples write no row; charts show
  gaps. Non-finite values are never stored.
- **Retention:** raw 24 h, then one-minute buckets keeping min/max/avg/count for
  7 days; compaction at startup and hourly, raw rows deleted only in the same
  transaction as their aggregate.
- **Bounded queries:** `get_metric_history(metrics, range)` for 15m/1h/6h/24h/7d,
  at most ~720 points per series; `latest` gives the exact last sample.
- **Events:** `history-sample-recorded { batchId, timestampMs, rowCount }` after
  each batch; the UI re-queries only visible panels. `get_history_status` reports
  cadence, timings and (on demand) row counts and file sizes.
- **Time handling:** UTC storage, monotonic scheduling, no catch-up after sleep
  or clock steps, gaps above 3× the bucket, injected clock for tests.
- **Privacy:** source identifiers are stored as a digest (`kind:` + 64-bit
  SHA-256), never a MAC or serial; rows are key, timestamp and number only.
- **Clean shutdown:** on exit the scheduler stops and the WAL is checkpointed.
- **Generic visualization engine** (`src/visualization/`): `MetricVisualization`
  renders any metric from `data + meta + config` inside a W×H box. Six renderers
  — line, area, sparkline, value, bar, gauge — from one data path; typed,
  serialisable `VisualizationConfig` (size, line, curve, markers, fill,
  colours, colour modes theme/manual/threshold, per-series colours and labels,
  background and opacity, border, radius, shadow/glow, text scale/weight,
  decimals, axes, grid, fixed/auto scale, visual-only smoothing, legend,
  tooltip, summary statistics, compact mode, gauge arc and thickness).
- **Presets:** Clean, Minimal, Technical, Gaming, Compact, Neon, Transparent —
  starting points, never locks; *Custom (from …)*, *Reset visualization*, *Save
  as preset*.
- **One Customize panel** for every chart, with a live preview and native colour
  pickers; choices persist in versioned `localStorage` (`pulse.visualization.v1`).
- **History sections** on the Overview: CPU (Total = `cpu.usage.total`, plus a
  small-multiples logical-processor view), memory, thermal, GPU, storage
  read/write, network download/upload, process counts, and a History recorder
  card.
- `d3-shape` (path generation only, ~2.5 KB gzip) is the one new frontend
  dependency.

### Fixed — Rust 1.77.2 compatibility of the whole build

- **The declared MSRV now holds for the entire locked dependency graph**, not
  only for PULSE's own code. Verified by building, testing and checking with
  the real 1.77.2 toolchain — see `docs/development/msrv.md`.
- `rusqlite` 0.40 → **0.32.1** (`libsqlite3-sys` 0.38.2 → 0.30.1, bundled
  SQLite 3.46.0): the newest line that builds on 1.77.2 (0.33+ uses
  `#[expect]`, Rust 1.81). No API change was needed; schema, cadence,
  retention and queries are unchanged.
- `windows-version` 0.100 → **0.1.7** (0.100 is edition 2024, Rust 1.95) —
  a Windows-only direct dependency that was already above the MSRV before
  Phase 10. Same `OsVersion::current()` API.
- The lockfile was re-resolved with Cargo's MSRV-aware resolver (484 → 470
  packages: 89 changed version, 20 removed, 13 added) and three crates
  that declare no `rust-version` were pinned by hand: `dlopen2` 0.8.0 /
  `dlopen2_derive` 0.4.0 (0.4.2+ are edition 2024), `getrandom` 0.3.3 and
  `wasi` 0.14.2 (wasm-only, kept clean so the lockfile has no edition-2024 or
  post-1.77.2 manifest at all). Tauri stays on 2.11.
- CI: a new `msrv` job builds and tests `src-tauri` with Rust 1.77.2 and
  `--locked` on Linux **and Windows** — on Windows that is also the first real
  MSVC compilation of the bundled SQLite. Not yet run (branch not pushed).
- Docs no longer imply that bundled SQLite was built for Windows from Fedora.

### Changed

- The storage providers (Fedora and Windows) read NVMe health only when a
  `storage.health.*` key of that device is requested, so frequent I/O sampling
  never wakes the controller.
- The app is now built with `Builder::build` + `run` so history can stop cleanly
  on `RunEvent::Exit`.

### Documentation

- New `docs/history/{architecture,storage,retention}.md` and
  `docs/visualization/{architecture,renderers,customization}.md`, including the
  Phase 11 rendering contract. Updated the architecture overview, metrics
  README, Fedora and Windows platform docs (Windows manual protocol · NOT
  EXECUTED) and README.

### Added — Phase 9: Process inspector, provenance, context actions & controls

- **Three services, kept apart.** `ProcessSnapshotService` (the table, every
  Refresh), `ProcessInspectorService` (one process, lazily) and
  `ProcessControlService` (explicit actions). None is a metric provider;
  provider count stays 6 and the catalog does not grow.
- **Process Inspector** drawer: identity, parent, instance ID, real start time,
  owner, category (User / System / Kernel thread / Unknown), architecture (ELF
  header on Linux, `IsWow64Process2` on Windows), executable metadata, priority,
  affinity, capabilities with reasons.
- **Provenance**: Fedora RPM owner via a bounded, shell-free `rpm -qf --`;
  Windows Authenticode via `WinVerifyTrust` (embedded, then catalog), publisher,
  and version resource. Offline: revocation off, cache-only URL retrieval. No
  safe/malware verdict anywhere.
- **SHA-256** on demand only, streamed in 64 KiB chunks through `/proc/<pid>/exe`
  on Linux, with `changedWhileHashing` when the file changes under the read.
- **Search online / Search hash online / Check hash on VirusTotal** open the
  browser on explicit click only, with program names or the digest — never a
  path, user name, PID or file upload. *Open file location* via
  `tauri-plugin-opener` (Rust side only).
- **Context menu** (right click, Shift+F10, Menu key) with Copy, Suspend,
  Resume, End process, End process tree, Force kill (Linux), Set priority, Set
  affinity; disabled items show their reason. Application rows get Inspect /
  Search / Show processes only.
- **Controls**: Linux pidfd-pinned `SIGTERM`/`SIGKILL`/`SIGSTOP`/`SIGCONT`,
  per-thread nice and affinity; Windows handle-pinned `TerminateProcess`,
  per-thread `SuspendThread`/`ResumeThread` limited to PULSE's own suspensions,
  `SetPriorityClass` (Realtime confirmed and read back), `SetProcessAffinityMask`
  (> 64 CPUs read-only). **PID-reuse guard**: every action re-validates PID and
  start token and answers `staleProcess` rather than touching a newcomer.
  End process tree: deepest first, each member re-validated, bounded single pass.
- Structured `ProcessActionResult` (`success`, `permissionDenied`,
  `staleProcess`, `processGone`, `unsupported`, `partialFailure`,
  `invalidRequest`, `platformError`). Confirmations for End / End tree / Force
  kill / Realtime. One Refresh after each action, never polling.
- Category filter (All / User / System / Kernel); search placeholder
  "Search by name or PID".

### Fixed — Phase 9

- **Sorting now reverses.** Phase 8's headers never toggled direction. Every
  numeric column opens descending and flips on each click, Name opens A → Z;
  ↑/↓ and `aria-sort` on the active column; unavailable values stay last in both
  directions; the choice survives Refresh. PID, Threads and application Processes
  columns are now sortable. Component tests click real headers three times in
  both tables.
- **Windows I/O semantics.** Docs no longer claim `ReadTransferCount` /
  `WriteTransferCount` equal Linux block-level `read_bytes` / `write_bytes`.


### Added — Phase 8: Processes & applications

What is running on this machine, what it is using, and — the decision this
phase is really about — **which of it belongs in the metric catalog and which
does not**. PULSE observes processes; it does not control them.

#### The architectural decision

A desktop runs three to five hundred processes, most of them for under a
second. A `MetricDefinition` is a promise that a saved dashboard reference
still resolves months later, and `process:1234-9001` stops resolving the moment
that process exits. Six metrics per PID would be roughly **two thousand
definitions replaced wholesale every refresh**, would push thousands of
disposable references through a `sample_metrics` call shaped for tens, and
would make `metricCount` a number that says nothing about the machine.

So the family is split in two:

- **`linux.processes` / `windows.processes`** publish exactly three
  low-cardinality metrics on `process:system`: `process.count.total`,
  `process.count.running` and `process.thread.count.total`. Three, whether the
  machine runs 180 processes or 900.
- **`ProcessSnapshotService`** serves the several hundred rows through
  `get_process_snapshot`, a command of its own, and discards them.

The separation is asserted, not merely documented: one test takes two snapshots
and proves the catalog did not grow, another proves neither platform declares a
source other than `process:system`.

- **Provider count goes to 6.** The catalog becomes
  `16 + 3N + P + 11G + 13D + 4V + 11I + 4W` — **314** on the reference machine,
  and the same 314 with 900 processes running. Note what is absent from that
  formula: the number of processes.
- **`process` becomes a canonical `SourceId` kind**, with exactly one
  registered instance.

#### Process identity: PID plus start time

PIDs are recycled. PID 1234 can be Firefox with 812 seconds of CPU time at
10:00:00 and a freshly started `cargo` at 10:00:04; a tracker keyed on the PID
alone differences 0.2 s against 812 s and either hides real work or invents a
10 000 % spike.

Identity is therefore `(pid, start_token)` — field 22 of `/proc/<pid>/stat` on
Fedora, `GetProcessTimes`'s `ftCreationTime` on Windows. The token is never
interpreted as a time, only compared. Two tests pin the behaviour: a recycled
PID inherits neither a CPU nor an I/O baseline.

#### One CPU convention on both platforms

`0–100 %` is a share of the machine's **entire** capacity, so one thread
saturating one of 32 logical processors reads `3.125 %`. This is the only
convention under which `sum(process CPU) ≈ cpu.usage.total`, which is what
makes the process list *explain* the system gauge rather than contradict it —
instead of the usual `3200 %` on Linux and `100 %` on Windows for the same
work. Measured on Fedora: a spawned `yes` reported **3.1215 %**.

Linux clock ticks and Windows 100 ns `FILETIME` intervals are both converted to
nanoseconds by their collector, so the delta arithmetic is one shared function.
`cutime`/`cstime` are excluded: a reaped build's CPU time does not belong to
the shell that launched it.

#### Per-process data

| Value                                 | Fedora                                    | Windows                       |
| ------------------------------------- | ----------------------------------------- | ----------------------------- |
| Inventory, parent, threads, name      | `/proc`, `/proc/<pid>/stat`               | `CreateToolhelp32Snapshot`    |
| CPU time and start token              | fields 14, 15, 22                         | `GetProcessTimes`             |
| Resident memory                       | field 24 × page size (RSS)                | `K32GetProcessMemoryInfo` (working set) |
| Storage I/O                           | `/proc/<pid>/io` `read_bytes`/`write_bytes` | `GetProcessIoCounters` transfer counts |
| Executable, for grouping              | `/proc/<pid>/exe`                         | `QueryFullProcessImageNameW`  |
| Classification                        | `PF_KTHREAD`, owner of `/proc/<pid>`      | PID 0/4, `%SystemRoot%`       |
| State                                 | field 3                                   | **unsupported** — see below   |

- **`read_bytes`, never `rchar`.** `rchar` counts every byte passed to
  `read(2)`, including page-cache hits, pipes, sockets and `/proc` itself;
  using it would report gigabytes per second off a disk that never moved, and
  would contradict PULSE's own `storage.io.*` metrics on the same screen. The
  price is that purely cached, tmpfs or network I/O reads `0 B/s`, which is the
  correct answer to "what is this doing to my storage". Verified: the same
  64 MiB write reports `0 B/s` on tmpfs and `1750.3 MiB/s` on btrfs.
- **RSS and Working Set are documented as the same *notion*, not the same
  mechanism**, and neither counts shared pages once — so an application's
  memory sum over-counts shared libraries, which is stated rather than silently
  corrected.
- **`process.count.running` is `unsupported` on Windows.** Windows schedules
  threads; a process has no state of its own. Synthesising one from thread
  states would make the same column mean two different things on the two
  platforms, so PULSE declares the metric with a reason instead of faking it.

#### A `/proc/<pid>/stat` parser that does not lie

Field 2 is `comm`, wrapped in parentheses and **not escaped**: `Web Content`,
`foo (bar)` and `kworker/3:1H-events` are all real names. A naive
`split_whitespace` does not fail — it *succeeds* with the wrong columns, giving
a `starttime` that changes every refresh and a CPU column permanently blank for
exactly the processes the user cares about. PULSE splits on the **last** `)`,
and a test asserts the naive approach genuinely disagrees.

#### Application aggregation

Processes are grouped by **executable identity** where a path is readable and
by process name otherwise, with the lower confidence marked in the interface.
Grouping by name alone would fold two unrelated `python3` programs into one row
whose CPU total means nothing. Sums are of what was measured: if two of five
processes are refused, the total is the three, and if nothing was measured the
reason is carried up rather than replaced by `0`.

#### Privacy and safety

- **No command lines, arguments or environment variables** are collected. They
  carry paths, URLs, connection strings and tokens; Phase 8 has no use for
  them. A test asserts the wire format has no such field.
- The executable path is collected because it is the only trustworthy grouping
  key, shown only as a tooltip, and never enters a `SourceId`.
- **No kill, suspend, resume, renice, debug, memory read or code injection**,
  and no code path that could.
- **No elevation.** Unprivileged Fedora loses `/proc/<pid>/io` and
  `/proc/<pid>/exe` for other users' processes — 125 of 702 on the reference
  machine — and every one of those rows keeps its other columns.
- On Windows, every handle is RAII-wrapped and closed before the next process
  is read. No handle survives a refresh; the baselines hold an identity and two
  integers.

#### Interface

A **Process details** card under Network, with two views of one snapshot:

- **Applications** (default) — Application, Processes, CPU, Memory, Read, Write.
- **Processes** — Process, PID, State, CPU, Memory, Threads, Read, Write, with
  the top 20 shown and a *Show all N processes* control. No nested scrollbar:
  the page scroll stays the only vertical scroll in the window.
- Sorting on CPU, Memory, Read, Write or Name, with a deterministic
  name-then-PID tie-break so the several hundred rows sitting at exactly `0 %`
  do not reshuffle on every refresh.
- A local **Search processes…** filter over name, PID and application name.
- The first snapshot shows `—` and *waiting for another sample* for CPU, Read
  and Write, and real values for memory and threads; after a baseline, a
  genuine `0 %` or `0 B/s` is shown as such.
- **Still no polling**: one snapshot on mount, one per *Refresh*. The service
  deliberately takes no baseline at startup either, so no number comes from an
  interval the user never asked for.

#### Measured on Fedora

702 processes, 2 164 threads, **16–26 ms per snapshot** — roughly 1 900
syscalls, one sequential pass, no subprocess, no thread per process and no
per-process Tauri call. The snapshot reports its own duration, so a future case
for parallelism can be made from a measurement.

#### Windows

Compiled for `x86_64-pc-windows-msvc` and pure-tested on Fedora — the
cross-check harness now includes `processes/` and `services/` by path as well —
but **not executed on a physical Windows machine.**

### Added — Phase 7: Network interfaces, traffic & Wi-Fi quality

Network interfaces, their identity, delta-based traffic rates and Wi-Fi link
quality, on both platforms. This phase is deliberately **passive**: it reads
counters the operating system was already keeping and sends not one packet.

- **Two machine-wide metrics** — `network.interface.count` (count, state) and
  `network.interface.up_count` (count, gauge), on `network:system`.
- **Eleven metrics per interface** — `network.receive`/`transmit` for
  `.bytes_per_second`, `.packets_per_second`, `.errors_per_second` and
  `.dropped_per_second`; `network.link.receive_speed` and `.transmit_speed`;
  and `network.mtu`.
- **Four metrics per Wi-Fi interface** — `network.wifi.signal.quality`,
  `.signal.rssi`, `.link.receive_rate` and `.link.transmit_rate`, declared
  **only** on wireless interfaces.
- **A catalog still sized by the machine** — `2 + 11I + 4W` network metrics for
  `I` published interfaces and `W` radios, bringing the total to
  `13 + 3N + P + 11G + 13D + 4V + 11I + 4W`: **307** on the reference machine
  (32 threads, 1 package, 1 GPU, 2 disks, 6 filesystems, 12 interfaces, 1
  radio). Nothing hardcodes any of them.
- **Provider count goes to 5.** `linux.network` and `windows.network` each own
  five backends. Wi-Fi is a **capability**, never a `linux.wifi` beside it: a
  radio is one interface with one identity, and a second provider publishing
  about it would claim the same `SourceId` and be rejected by the engine.

Three additions to the shared contract, each preventing a specific confusion:

- **`packetsPerSecond`**, deliberately not `operationsPerSecond`. A disk
  operation and a network packet come from different subsystems and differ by
  orders of magnitude on the same machine; sharing an axis would invite
  comparing 900 IOPS with 900 packets/s as if they meant the same thing.
- **`bitsPerSecond`** for link capacity. A 1 Gbit/s link carrying 12 MiB/s is
  one number in bits and one in bytes, and conflating them is a factor-of-eight
  error that looks entirely plausible. The card renders capacity with decimal
  prefixes and traffic with binary ones, because that is what networking and
  storage each actually use.
- **`decibelMilliwatts`** for signal strength. A logarithmic, negative scale
  that must never be averaged arithmetically or rendered from zero.

Where the numbers come from:

| Layer            | Fedora                                 | Windows                              |
| ---------------- | -------------------------------------- | ------------------------------------ |
| Inventory        | `rtnetlink` `RTM_GETLINK`              | `GetIfTable2` / `MIB_IF_ROW2`        |
| Identity         | `IFLA_PERM_ADDRESS`                    | `PermanentPhysicalAddress`, `InterfaceGuid` |
| Traffic counters | `IFLA_STATS64`, in the same dump       | the same table                       |
| Link speed       | `/sys/class/net/<iface>/speed`         | `ReceiveLinkSpeed`, `TransmitLinkSpeed` |
| Addresses        | `rtnetlink` `RTM_GETADDR`              | not read in this phase               |
| Which are Wi-Fi  | `nl80211` `GET_INTERFACE`              | `NDIS_PHYSICAL_MEDIUM`               |
| Wi-Fi link       | `nl80211` `GET_STATION`                | WLAN realtime connection quality     |

What it deliberately refuses to publish, each enforced by a test:

- **`0 B/s` before a baseline exists.** Traffic is a rate: the first sample has
  nothing to difference against and says so, rather than reporting an
  unmeasured interface as idle. Over a real interval, `0 B/s` and `0 pkt/s`
  *are* published, because they are measurements.
- **A spike after a counter reset.** An interface bounced down and up, a driver
  reset, a recreated virtual interface or Windows's 32-bit packet counters
  wrapping all make a total go down; any single field regressing restarts the
  baseline.
- **A quality percentage derived from dBm.** Every common formula is arbitrary:
  the usual one reads an ordinary −55 dBm link as 90 % and a marginal −85 dBm
  link as 30 %, and Windows's own mapping differs again. PULSE publishes a
  percentage only where the platform computes one — so Windows has it and
  Fedora reports `unsupported` with that reason, alongside a real RSSI.
- **An averaged multi-link RSSI, or `links[0]`.** dBm is logarithmic, so the
  mean of −50 and −90 is not −70; PULSE publishes the **strongest active
  link's** reading and says so.
- **`0 bit/s` as a link speed.** Fedora reports `-1`/`EINVAL` and Windows `0`
  and sometimes `u64::MAX` to mean "nothing negotiated"; none is published,
  because a link speed of zero claims a connection with no capacity.
- **Only the unicast half of a Windows packet count.** `InUcastPkts` alone
  silently drops every broadcast and multicast frame; the total is
  `Ucast + NUcast`, with checked arithmetic.
- **A drop counter presented as packet loss.** `rx_dropped` counts frames this
  machine discarded *after receiving them*, and says nothing about packets lost
  on the Internet. Errors and drops are also kept apart: a broken frame and an
  intact one nobody wanted are different counters.
- **A kind guessed from a name.** Every Wi-Fi station reports `ARPHRD_ETHER` on
  Linux and frequently `IF_TYPE_ETHERNET` on Windows, exactly like a wired NIC.
  PULSE asks `nl80211` and `NDIS_PHYSICAL_MEDIUM` instead.
- **Loopback as a user interface.** Always present, only ever carrying traffic
  that never left the machine. Excluded in the *shared* declaration code, so
  neither platform can forget and the two cannot disagree.

Also in this phase:

- **Identity survives MAC randomisation.** Both NetworkManager and Windows
  randomise a Wi-Fi interface's current MAC per network by default, so an
  identity built on it would change every time the user moved between home and
  the office. PULSE prefers the **permanent** address — `IFLA_PERM_ADDRESS`,
  `PermanentPhysicalAddress` — which is six bytes burned into the adapter and
  **identical on both operating systems**, so the same radio yields
  `network:mac-9009df3e97f2` on Fedora and on Windows. A contract test asserts
  it. `eth0`, an interface index, and an IP address are never identities; an
  all-zero permanent address, which every tunnel reports, is refused rather
  than collapsing every VPN onto one identifier.
- **No SSID, no BSSID, no location permission.** On Windows the obvious source
  for a signal reading also returns the network's identity, which makes the
  call subject to the machine's location permission. PULSE uses
  `wlan_intf_opcode_realtime_connection_quality`, which carries neither — and
  **does not fall back** to the location-gated call when an older Windows lacks
  it, because a monitoring tool should not reach for a location interface
  behind the user's back to get a number it just said it could not get.
- **Read-only, and no subprocesses.** The netlink socket joins no multicast
  group and can express only `RTM_GETLINK`, `RTM_GETADDR` and two `nl80211`
  reads. No `ip`, `ifconfig`, `ethtool`, `iw`, `iwconfig`, `nmcli`,
  `networkctl`, `PowerShell`, `netsh`, `wmic` or `ipconfig`, anywhere. Nothing
  configures an address, changes a link, joins a network or sends a packet.
- **Netlink is parsed in PULSE, with every bound checked.** The alternative
  crates are large, asynchronous, or a stack of five; what PULSE needs is one
  attribute iterator and two header structs, shared by both families. The
  payoff is that the fuzz-shaped cases are unit tests: a zero-length attribute
  that would hang a naive walker, one claiming more than the buffer holds, a
  truncated message, a multipart reply that never terminates, a stale reply
  from a previous request. A malformed message becomes a `MetricError`, never a
  crash.
- **One transaction per refresh, not one per metric.** On the reference machine
  a refresh is **three netlink round trips and one file read** for thirteen
  interfaces; the `/sys/class/net/*/statistics/` shape would be 104 file opens
  at 104 different instants — and rates need one instant.
- **Windows memory is freed through RAII guards.** `FreeMibTable` and
  `WlanFreeMemory` run on every path out, including the error ones, and the
  interface rows are copied out before the table is released so no raw pointer
  leaves the FFI.
- **Windows parsing is tested on Fedora.** `MIB_IF_ROW2` is declared as a
  `#[repr(C)]` struct with a compile-time assertion pinning its documented
  1352-byte size — the compiler computes the offsets — and the kind mapping,
  state mapping, counter arithmetic, WLAN response parsing and identity rules
  are all pure functions over synthetic structures.
- **UI** — a new *Network details* card: interface and connected counts, then a
  compact grid of adapters with their kind, state, MTU, addresses, eight
  traffic rows and, for a radio, four Wi-Fi rows. Bridges and container links
  are collapsed behind *Show all (N)* so a machine running containers does not
  bury its two real adapters — collapsed, never hidden. Before a baseline
  exists the card says *Waiting for another sample* rather than showing zeros,
  and fires no hidden second request. Still one sample on mount and one per
  Refresh: no polling.
- **Documentation** — [`docs/metrics/network.md`](docs/metrics/network.md).

**Deliberately out of scope**, and stated as such: ping, round-trip latency,
jitter, Internet packet loss, DNS timing, throughput capacity tests, public-IP
lookup and geolocation. Each needs an active probe against a chosen target,
with a cadence and a privacy decision attached. Keeping them out is what lets
the passive foundation ship without one.

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
