# Metric Providers

> Phase 1 defined the provider contract and the engine that hosts it. Phase 2
> added the first real providers — CPU and memory, natively on both platforms.
> Phase 3 grew the CPU providers to cover every logical processor. Phase 4 added
> a GPU provider per platform, each hosting several vendor backends behind one
> owner. The contract has not had to change once.

## What a provider is

A provider is the unit of ownership in the metrics engine: it declares a set of
metrics and knows how to read them. One provider maps onto one data source.

**A provider owns a family of metrics, not a single device.** `linux.cpu` owns
every CPU metric on the machine — the aggregate, the topology counts, and three
metrics for each of the 32 logical processors. There is deliberately **no
provider per processor**: thirty-two providers would each re-read `/proc/stat`,
each appear in the engine status, and share nothing. So `Providers = 2` holds on
both platforms whatever the CPU; it is the **metric count** that scales with the
machine.

The same reasoning decides the GPU architecture, more sharply. `linux.gpu` owns
the DRM inventory, the NVML capability and the AMDGPU capability together —
because an NVIDIA card is seen by _both_ the DRM inventory and NVML, so
registering `nvidia.nvml` separately would make two providers claim the same
`MetricRef` and the engine would reject one. Owning the family in one provider
is what lets the backends be merged before anything is published.

```text
linux.cpu      /proc/stat, /sys/devices/system/cpu
linux.hwmon    /sys/class/hwmon
windows.pdh    Performance Data Helper counters
windows.wmi    WMI queries (inventory only — too slow to poll)
nvidia.nvml    NVIDIA Management Library
storage.smart  SMART attributes (privileged)
```

The engine knows nothing about any of that. It knows about providers, a catalog
and references — which is what makes this true:

> All future PULSE metrics can be added behind a single contract without the
> interface needing to know whether they come from Fedora, Windows, NVIDIA,
> `/proc`, WMI or SMART.

## The trait

```rust
pub trait MetricProvider: Send + Sync {
    fn id(&self) -> &ProviderId;
    fn describe(&self) -> Result<Vec<MetricDefinition>, MetricError>;
    fn sample(&self, requested: &[MetricRef]) -> Result<Vec<MetricSample>, MetricError>;
}
```

### Why synchronous

The interfaces PULSE will read — `/proc`, `/sys`, PDH, WMI, NVML — are blocking
calls. Wrapping them in `async` would add a runtime and an `async_trait`
allocation per call without removing a single blocking operation. Scheduling and
concurrency are the **engine's** concern in a later phase, not the provider's; a
provider stays a plain, testable function from references to samples.

### Why `Send + Sync`

The engine is shared across Tauri commands today, and across background
samplers and the Mini overlay window later. `Arc<MetricsEngine>` requires it.

### Responsibilities

1. **Declare everything, including what is missing.** A metric that exists
   conceptually but cannot be read here should still be declared, with a
   non-available `availability` explaining why. That is what lets the UI say
   _"your board exposes no fan sensor"_ rather than silently omitting it.
2. **Never panic.** Every foreseeable failure has a representation: an
   unavailable `MetricSample` for one metric, `Err(MetricError)` for the whole
   provider.
3. **Return canonical units.** Hertz, bytes, Celsius — see
   [`model.md`](model.md#units--the-canonical-rule). Converting `/proc`'s kB or
   hwmon's millidegrees is the provider's job, not the widget's.
4. **Sample only what was asked for.** The engine passes the subset it needs,
   already deduplicated.
5. **Own stable identity.** Derive `SourceId` instances from stable device
   identifiers, never from product names — see
   [`identifiers.md`](identifiers.md).

## Registration

```rust
let mut engine = MetricsEngine::new();
engine.register(Arc::new(LinuxCpuProvider::new()?))?;
```

Registration is **atomic**. If any declared metric collides with one already in
the catalog, or the provider declares the same reference twice, _nothing_ is
added — a partly registered provider would be much harder to reason about than
a rejected one. This is tested.

`RegistrationError` distinguishes four cases:

| Variant                   | Meaning                                      |
| ------------------------- | -------------------------------------------- |
| `DuplicateProvider`       | That provider id is already registered       |
| `MetricCollision`         | Two providers claim the same `MetricRef`     |
| `DuplicateWithinProvider` | One provider declared a reference twice      |
| `Describe`                | The provider could not enumerate its metrics |

Collisions are never silent. Two providers quietly fighting over
`cpu.temperature.package` would produce readings that change depending on
registration order — the kind of bug that takes days to find.

At startup, a provider that fails to register should be **logged and skipped**,
not propagated: one unavailable provider must never prevent PULSE from starting.

## Isolation

The engine guarantees, all of it tested:

- **One sample per requested reference, in request order.** Callers can zip the
  response against their request.
- **Each provider is called at most once per request**, with its references
  deduplicated — asking for five GPU metrics does not query the driver five
  times.
- **A failing provider only affects its own metrics.** GPU provider down, CPU
  and network results still returned.
- **Unknown references are answered, not rejected** — they come back as
  `notRegistered`, so one stale dashboard entry cannot blank out every other
  widget.
- **Missing samples are accounted for.** A provider that omits a metric it
  declared yields a `missingSample` error rather than a silent gap.
- **An empty request touches no provider.**

### Panics

The engine catches _errors_, not _panics_. `src-tauri/Cargo.toml` sets
`panic = "abort"` in the release profile, so `catch_unwind` would work in debug
and do nothing in release — an isolation guarantee that silently evaporates in
the shipped binary is worse than none.

The contract therefore requires providers not to panic, and the trait gives
every failure a return value instead. **Whether to drop `panic = "abort"` and
sandbox provider calls is a deliberate open decision**, worth revisiting when
the first providers touch genuinely flaky hardware interfaces.

## Testing a provider

Parsing and interpretation must be **pure functions taking their input as
arguments** — `parse_proc_stat`, `parse_meminfo`, `counters_from_system_times`,
`filetime_to_u64` — so they can be exercised with fixtures instead of against a
live kernel.

Both platform modules are compiled on **every** host; only the FFI calls and the
`HostPlatform` implementations are gated behind `cfg(target_os)`. A Fedora test
run therefore exercises the Windows arithmetic, and a Windows run exercises the
`/proc` parsers. There is no reason for Windows maths to be untestable from a
Linux machine.

Host-specific tests — those reading the real `/proc` or calling the real API —
are gated to their platform and assert **invariants only**: never a particular
amount of RAM or a particular load, which would make CI depend on the runner's
hardware.

The engine itself is tested with `MockProvider`
(`src-tauri/src/metrics/providers/mock.rs`), which is `#[cfg(test)]` and
therefore never compiled into the shipped binary.

## Cross-platform rule

> A system-facing feature is not considered complete until its behavior on both
> Windows and Fedora Linux has been designed and, whenever materially testable,
> validated.

For a provider this means: a metric family implemented on one platform must, on
the other, either be implemented or be **declared with an explicit
`unsupported` availability and a reason**. Silence is not an acceptable
cross-platform answer.

## Providers

| Provider         | Platform         | Data source                                                                                                  | Status          |
| ---------------- | ---------------- | ------------------------------------------------------------------------------------------------------------ | --------------- |
| `linux.cpu`      | Fedora           | `/proc/stat`, `/sys/devices/system/cpu`, `hwmon` (package temperature)                                       | **Implemented** |
| `linux.memory`   | Fedora           | `/proc/meminfo`                                                                                              | **Implemented** |
| `linux.gpu`      | Fedora           | `/sys/class/drm`, NVML, `amdgpu` sysfs, the card's own `hwmon` node                                          | **Implemented** |
| `windows.cpu`    | Windows          | `GetSystemTimes`, `NtQuerySystemInformationEx`, `CallNtPowerInformation`, `GetLogicalProcessorInformationEx` | **Implemented** |
| `windows.memory` | Windows          | `GlobalMemoryStatusEx`                                                                                       | **Implemented** |
| `windows.gpu`    | Windows          | DXGI, NVML                                                                                                   | **Implemented** |
| `windows.pdh`    | Windows          | Performance counters                                                                                         | Planned         |
| `storage.smart`  | both, privileged | SMART                                                                                                        | Later           |

There is deliberately **no `linux.hwmon` provider**. A CPU's thermal sensors and
its usage counters describe the same `cpu:*` sources, and a GPU's sensors and its
counters describe the same device — two providers would claim the same
`MetricRef` and the engine would reject one of them by design. `hwmon` is a
_capability_ of the CPU and GPU providers, not an owner of metrics, which is the
same reason NVML is not registered separately. See
[`thermals.md`](thermals.md).

See [`cpu-memory.md`](cpu-memory.md), [`cpu-advanced.md`](cpu-advanced.md),
[`gpu.md`](gpu.md) and [`thermals.md`](thermals.md) for how the implemented ones work, and
[`../platforms/fedora.md`](../platforms/fedora.md) /
[`../platforms/windows.md`](../platforms/windows.md) for the data sources and
their constraints.

## How providers reach the engine

The platform layer decides which providers exist; the engine only hosts them.

```text
HostPlatform::metric_providers()     platform/linux/mod.rs, platform/windows/mod.rs
        │  Vec<Arc<dyn MetricProvider>>
services::metrics::build_engine()    the composition point
        │
metrics::build_engine(providers)     registers each; logs and skips failures
        │
MetricsEngine                        no cfg(target_os) anywhere inside
```

`services` sits above both `metrics` and `platform`, so neither depends on the
other and the engine never learns that Linux or Windows exist. Adding a provider
means implementing the trait and returning it from `metric_providers()` — no
change to the engine, and no new `cfg` outside the platform layer.

A provider that fails to register is **logged and skipped**, never propagated:
one unavailable data source must not stop PULSE from starting.

## Sharing declarations between platforms

`metrics/wellknown/` owns each shipped metric's key, source, unit, category,
kind and user-facing text, plus the arithmetic that turns raw counters into the
published value. Platform providers supply **only the raw numbers** — they never
choose a key, a unit or a formula.

That is what makes `memory.used@memory:system` mean exactly the same thing on
both operating systems, and a contract test asserts that the Linux and Windows
declarations differ in `providerId` and in nothing else.
