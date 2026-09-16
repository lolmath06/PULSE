# GPU Metrics

> Phase 4. GPU inventory, stable identity, and the core telemetry contract, on
> both Fedora Linux and Windows across NVIDIA, AMD and Intel hardware.

PULSE answers:

> Which GPUs does this machine have, what is their stable identity, what load
> and memory are they using, and at what frequencies does the OS or driver
> report them running?

And — just as importantly — it answers honestly when it cannot.

## Four problems, not one

| Problem                             | Solved by                                      |
| ----------------------------------- | ---------------------------------------------- |
| Discovering the GPUs                | DRM on Fedora, DXGI on Windows                 |
| Giving each a stable identity       | NVML UUID, else PCI address, else device model |
| Publishing one common contract      | `metrics/wellknown/gpu/`                       |
| Reading what is genuinely available | NVML and AMDGPU capabilities, per metric       |

Vendors differ enormously in what they expose, so all three of these are normal
and none hides the device:

```text
GPU recognised + every metric available          an NVIDIA card with its driver
GPU recognised + some metrics available          an AMD card whose kernel exposes only some files
GPU recognised + no telemetry at all             an NVIDIA card running nouveau
```

## The metrics

Machine-wide, on `gpu:system`:

| Metric      | Unit    | Kind  | Value type |
| ----------- | ------- | ----- | ---------- |
| `gpu.count` | `count` | state | number     |

Per GPU, on that GPU's own source:

| Metric                     | Unit      | Kind      | Value type |
| -------------------------- | --------- | --------- | ---------- |
| `gpu.usage.core`           | `percent` | gauge     | number     |
| `gpu.memory.total`         | `bytes`   | **state** | number     |
| `gpu.memory.used`          | `bytes`   | gauge     | number     |
| `gpu.memory.free`          | `bytes`   | gauge     | number     |
| `gpu.memory.usage.percent` | `percent` | gauge     | number     |
| `gpu.frequency.core`       | `hertz`   | gauge     | number     |
| `gpu.frequency.memory`     | `hertz`   | gauge     | number     |

`gpu.count` and `gpu.memory.total` are **states, not gauges**, for the same
reason `cpu.count.*` is: installed VRAM and the number of cards are hardware
facts, not readings that rise and fall. `MetricKind::State` is not directly
averageable, so the aggregation rules arriving with history will refuse to
produce "1.4 GPUs" or "7.3 GiB of installed VRAM" by construction.

## The catalog is sized by the machine

```text
1 + 7G      for G discovered GPUs
```

added to the CPU and memory families. On the development machine:

```text
100   CPU    (8 + 3 × 32 logical processors)
  4   memory
  8   GPU    (1 + 7 × 1)
────
112   total
```

Nothing hardcodes `G`. A headless server publishes just `gpu.count` reporting
zero — which is a fact, not a failure.

**An unsupported metric keeps its definition.** A GPU whose driver exposes no
clock still declares `gpu.frequency.core@<its source>` with an `unsupported`
availability and a reason. It is never dropped. That is what lets a dashboard
built on a machine with full telemetry open on one without it and explain
itself, and start working again when a proper driver is installed.

## GPU identity — the hard problem

Reading a percentage is easy. Deciding _which GPU it belongs to_, in a way that
still means the same thing after a reboot, a driver update or a second
identical card, is not. A dashboard stores a `SourceId`; if that shifts, the
user's GPU widgets silently start showing a different card.

### What must never be an identity

| Candidate              | Why not                                                                                                                                          |
| ---------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------ |
| `GPU 0`, `GPU 1`       | Presentation ordering, nothing more                                                                                                              |
| `/sys/class/drm/card0` | A DRM **minor number**, assigned in probe order. `card0` and `card1` can swap between boots, and a card removed and re-probed can become `card3` |
| **NVML index**         | NVIDIA documents it as unstable across reboots; it also changes when a GPU is added, removed or reset                                            |
| **DXGI adapter index** | Enumeration order, reflecting which adapter Windows currently prefers                                                                            |
| `AdapterLuid`          | Microsoft documents it as valid **only until the system restarts**                                                                               |
| Product name           | Two identical cards collapse into one identifier, and a driver update that rewords the string invalidates every saved widget                     |

All three indices above are used — but only _within the current enumeration_,
never stored.

### What PULSE uses instead

| Mechanism        | Source                             | Stability                                                  | Used when                                              |
| ---------------- | ---------------------------------- | ---------------------------------------------------------- | ------------------------------------------------------ |
| **NVML UUID**    | `nvmlDeviceGetUUID`                | `Hardware`                                                 | Any NVIDIA GPU with the driver installed, on either OS |
| **PCI address**  | DRM `device` symlink               | `Slot`                                                     | Everything else on Fedora                              |
| **Device model** | DXGI vendor/device/subsys/revision | `Slot`, or `ModelWithSessionDisambiguator` when duplicated | Everything else on Windows                             |

```text
gpu:system                                      the machine's GPUs together
gpu:nvidia-11111111-2222-3333-4444-555555555555 NVML UUID
gpu:pci-0000-01-00-0                            PCI address
gpu:amd-73ff-10020e3b-c1                        device model
gpu:amd-73ff-10020e3b-c1-n1                     device model, session-disambiguated
```

The stability level is **recorded in the descriptor**, not assumed, because the
honest answer differs by platform and vendor:

- `Hardware` — survives reboots and tells two identical cards apart.
- `Slot` — survives reboots and tells identical cards apart, but changes if the
  card is physically moved to another slot.
- `ModelWithSessionDisambiguator` — survives reboots, but **two identical cards
  are told apart only within one session**. This is the honest ceiling of what
  DXGI alone can promise, and PULSE does not claim otherwise.

The product name lives in `sourceLabel` and is presentation only. When two cards
share a name the interface appends `#1`, `#2` for display — without touching the
identity underneath.

## Provider architecture

**One provider owns every GPU metric on the machine.**

```text
linux.gpu                              windows.gpu
 ├── generic inventory  /sys/class/drm   ├── generic inventory  DXGI
 ├── NVIDIA capability  NVML             └── NVIDIA capability  NVML
 └── AMD capability     amdgpu sysfs
```

Registering `linux.gpu`, `nvidia.nvml` and `amd.sysfs` as three separate
providers is the obvious alternative and is wrong: an NVIDIA card is seen by
both the DRM inventory _and_ NVML, so two providers would claim the same
`MetricRef` and the engine would reject one outright. Owning the family in one
provider is what lets the backends be merged before anything is published.

```text
Providers = 3        cpu, memory, gpu
```

on both platforms, whatever the hardware. The **metric count** is what scales.

### Deduplication

A card visible to both the generic inventory and a vendor backend must be
published **once**, under the stronger identity.

- **By PCI address first.** DRM and NVML both report the bus address of the same
  slot, so this is exact — it distinguishes identical cards and cannot be fooled
  by naming. This is the Fedora path.
- **By vendor and enumeration order second.** DXGI exposes no bus address, so
  the _n_-th NVIDIA adapter is paired with the _n_-th NVML device. Best
  available, and recorded as such. This is the Windows path.
- **Never by product name.**

The vendor descriptor wins the merge: it carries a hardware UUID rather than a
slot address, and the telemetry the generic inventory cannot provide.

## NVIDIA — NVML

### Loaded at runtime, never linked

Phase 3's `NtQuerySystemInformationEx` fix established the rule:

```text
an optional monitoring backend  ≠  a mandatory application dependency
```

PULSE starts normally with an NVIDIA card, without one, with `nouveau` instead
of the proprietary driver, with the driver present but NVML missing, and with an
NVML too old to export a symbol. Linking NVML at load time would turn every one
of those into a process that refuses to start.

**Fedora**: `dlopen("libnvidia-ml.so.1")` — the SONAME, never a bare
`libnvidia-ml.so` (that symlink belongs to the CUDA _development_ package, which
most users do not have) and never an absolute path.

**Windows**: `LoadLibraryExW(L"nvml.dll", NULL, LOAD_LIBRARY_SEARCH_SYSTEM32)`.

The flag is the entire point. A plain `LoadLibraryW("nvml.dll")` searches the
**application directory first**, so anyone able to drop a file next to
`pulse.exe` could have PULSE load their DLL with PULSE's privileges — a classic
DLL planting vulnerability, and a monitoring tool that loads vendor libraries is
exactly the target. `LOAD_LIBRARY_SEARCH_SYSTEM32` restricts the search to
`%SystemRoot%\System32`, where the NVIDIA driver installs the library.

PULSE deliberately does **not** widen the search on failure. A missing NVML
costs one vendor's metrics; a hijacked NVML is arbitrary code inside PULSE.

### Why not a crate

`libloading` would do the loading and `nvml-wrapper` the whole job. Neither is
used: the Windows DLL search path is a security decision that belongs in PULSE's
own code rather than inherited from a dependency's defaults; PULSE already
resolves `ntdll` natively for the same reason; and a full vendor-monitoring
crate would decide the metric semantics on PULSE's behalf and bundle vendor
headers PULSE has no business shipping.

### Symbols resolved

Only what this phase needs:

```text
nvmlInit_v2                     nvmlDeviceGetUUID
nvmlShutdown                    nvmlDeviceGetName
nvmlDeviceGetCount_v2           nvmlDeviceGetPciInfo_v3
nvmlDeviceGetHandleByIndex_v2   nvmlDeviceGetUtilizationRates
                                nvmlDeviceGetMemoryInfo
                                nvmlDeviceGetClockInfo
```

Temperature, power, fan and encoder entry points exist in NVML and are
deliberately **not** bound — they belong to the sensors phase, and binding a
symbol PULSE does not use is one more thing that could fail at startup for no
benefit.

All of these are required; a library missing any is too old to serve this phase,
and saying so once at load time beats discovering it per metric.

### Granular degradation

An NVML failure is mapped by _kind_, never collapsed into "provider error":

| NVML            | Availability             | Meaning to the user                                      |
| --------------- | ------------------------ | -------------------------------------------------------- |
| `NOT_SUPPORTED` | `unsupported`            | This GPU or driver does not report it. Common and normal |
| `NO_PERMISSION` | `permissionDenied`       | Actionable                                               |
| `GPU_IS_LOST`   | `temporarilyUnavailable` | Expected to recover                                      |
| `NOT_FOUND`     | `notDetected`            | The device went away                                     |
| `UNINITIALIZED` | `providerError`          | PULSE's own fault, reported as such                      |

So this is a perfectly usable GPU:

```text
UUID                Available
GPU usage           Available
VRAM                Available
core frequency      Available
memory frequency    Unsupported
```

### Lifecycle and thread safety

`nvmlInit_v2` runs once; `nvmlShutdown` exactly once when the handle drops,
after which the library is unloaded. Device handles are cached inside the same
object and cannot outlive it, because every accessor borrows `&self` — so no
thread can use a handle while another shuts the library down. The provider is
`Send + Sync`, as the engine requires.

## Fedora

### Generic inventory — `/sys/class/drm`

`/sys/class/drm` mixes three kinds of entry, and only one is a device:

```text
card0              a DRM device        ← counted
card0-DP-1         a display connector ← skipped
card0-eDP-1        a display connector ← skipped
renderD128         a render node for card0, the same hardware ← skipped
version            not a device at all ← skipped
```

Counting connectors would report a four-output laptop as having five GPUs;
counting render nodes would double every card. An entry is a device only when
named `card<N>` with `<N>` numeric, and _hardware_ only when it resolves to a
real PCI device — which excludes `vkms`, `simpledrm` and other virtual devices.

Read per card: PCI address (from the `device` symlink target), vendor ID, device
ID and bound driver.

**Device names** come from the system PCI ID database (`/usr/share/hwdata/pci.ids`),
read as an ordinary file. This is the same data `lspci` prints, obtained without
running it. When the database is absent, names fall back to `NVIDIA GPU
10de:2820` — unhelpful, but never wrong.

### AMD — `amdgpu` sysfs

```text
/sys/class/drm/cardN/device/
├── gpu_busy_percent       engine utilisation, whole percent
├── mem_info_vram_total    dedicated VRAM, bytes
├── mem_info_vram_used     dedicated VRAM in use, bytes
├── pp_dpm_sclk            core clock states, active marked with '*'
└── pp_dpm_mclk            memory clock states, active marked with '*'
```

The VRAM files are already in bytes — unlike `/proc/meminfo`'s kB, there is no
conversion and so no chance of being wrong by 1024.

The `pp_dpm_*` files list clock states with the active one marked:

```text
0: 500Mhz
1: 1200Mhz *
2: 2100Mhz
```

The marker, spacing and capitalisation of `Mhz` all vary between driver
versions, so the parser is written against several real shapes. When no line is
marked — which happens during a power-state transition — PULSE reports nothing
rather than guessing the first or highest entry.

**An absent file means that metric is unavailable, never that the GPU is gone.**
These attributes come and go with driver version, power-management mode and ASIC
generation.

### Intel

Inventoried correctly; telemetry is `unsupported`. PULSE does not fabricate a
whole-GPU utilisation figure from a source that does not represent one. Exactness
before exhaustiveness.

### No subprocess

Never `nvidia-smi`, `lspci`, `glxinfo`, `vulkaninfo`, `radeontop`, `rocm-smi`,
`intel_gpu_top`, `cat`, `grep` or `awk`. Rust reads files and calls libraries
directly.

### No privileges

`/sys/class/drm` is world-readable and NVML's queries work for any user.

## Windows

### Generic inventory — DXGI

`CreateDXGIFactory1` + `IDXGIFactory1::EnumAdapters1`. No privileges, no
service, no vendor SDK — the same enumeration every Direct3D application
performs. `DXGI_ADAPTER_DESC1` supplies the description, vendor and device IDs,
subsystem, revision and dedicated video memory capacity.

DXCore was considered: newer and richer, but it targets compute-capable adapter
enumeration, is unavailable on older supported Windows versions, and nothing
this phase needs is missing from DXGI.

DXGI is COM, which `windows-sys` deliberately does not cover, so the `windows`
crate provides the bindings. It is already in the dependency graph on Windows
(Tauri pulls it in), so this adds correct COM lifetime handling and no build
weight.

### Software adapters are not GPUs

Windows always presents _Microsoft Basic Render Driver_ (WARP), and on a machine
with no display driver it may be the only adapter. PULSE excludes an adapter
when **either** signal says software: `DXGI_ADAPTER_FLAG_SOFTWARE`, or the
reserved Microsoft vendor/device pair `0x1414:0x008C` — because older drivers do
not always set the flag.

### Why `QueryVideoMemoryInfo` is not published as VRAM usage

`IDXGIAdapter3::QueryVideoMemoryInfo(...).CurrentUsage` looks like the answer to
`gpu.memory.used` and is not: it reports the video memory attributed to **the
querying process**. PULSE would be measuring its own consumption and labelling
it system-wide VRAM usage — reading near zero on an idle machine while a game
filled the card.

So a DXGI-only adapter publishes:

| Metric                                       | Published                                                         |
| -------------------------------------------- | ----------------------------------------------------------------- |
| `gpu.memory.total`                           | **Yes** — `DedicatedVideoMemory` really is the installed capacity |
| `gpu.memory.used` / `free` / `usage.percent` | **No** — `unsupported`                                            |
| `gpu.usage.core`, `gpu.frequency.*`          | **No** — no unprivileged system-wide source in DXGI               |

Showing `—` is better than showing a number that is confidently wrong.

An integrated adapter reporting zero dedicated memory gets `notDetected` on
`gpu.memory.total`: the adjacent `SharedSystemMemory` figure is an addressing
limit on system RAM, not video memory, and PULSE will not turn it into pretend
VRAM.

### NVIDIA on Windows

Identical to Fedora: same NVML backend, same seven metrics, same UUID identity.
The card appears **once** — merged with its DXGI entry rather than published
twice.

## VRAM semantics

These metrics describe **dedicated, local video memory**. They deliberately do
not describe system RAM an integrated GPU carves out of main memory, Windows'
"shared system memory" figure, or a per-process video memory budget.

```text
free          = total - used        (when the source reports only two)
usage_percent = used / total * 100
```

When a source reports all three (NVML), the reported `free` is preferred — the
driver knows about reservations PULSE does not — but only when the three are
mutually consistent. A source contradicting itself is refused rather than
silently reconciled.

Refused outright: a zero total, `used` above `total`, `free` above `total`, and
`used + free` exceeding `total` (with checked arithmetic, so an overflow cannot
wrap into looking consistent). Nothing published is ever `NaN`, infinite,
negative or above 100.

## Frequency

Hertz on the wire, always. NVML reports megahertz and AMD's `pp_dpm_*` files
report megahertz; both are converted in the platform layer, with overflow
checks. **Zero is rejected** — a powered-down clock domain reports `0`, and
publishing `0 Hz` would render as `0 GHz`, which a user reads as a claim that
the card has stopped.

## Multi-GPU

PULSE works with an iGPU alone, a dGPU alone, iGPU + dGPU, two discrete cards,
several NVIDIA cards, and AMD beside NVIDIA. The count is never hardcoded, and
`gpu.count` counts the hardware adapters actually inventoried.

Each GPU has its own references, unique and deterministic. Tests cover two
identical cards specifically, proving the product name is never the identity.

## Performance

A refresh does **not** re-enumerate PCI, reload NVML, re-resolve symbols, or
re-read static capabilities. Identity, names, capacities and capabilities are
built once at startup; only the numbers that move are re-read.

Each device is read **at most once per request**, however many of its seven
metrics were asked for — one NVML round trip (four calls: utilisation, memory,
two clock domains) or one sysfs pass, not seven. A device the request does not
touch is not read at all.

## Deliberately out of scope

No temperature, hotspot or memory-junction sensors; no power draw, limits,
energy, voltage or throttling; no fan speeds; no encoder or decoder utilisation.
NVML can read most of these today — they belong to the sensors phase.

Still no scheduler, no polling, no history. The interaction model remains
`request → sample → response`.
