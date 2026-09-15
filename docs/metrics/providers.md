# Metric Providers

> Phase 1 defines the provider contract and the engine that hosts it. **No real
> provider is implemented yet.**

## What a provider is

A provider is the unit of ownership in the metrics engine: it declares a set of
metrics and knows how to read them. One provider maps onto one data source.

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
arguments**, exactly as in Phase 0 (`detect_display_server`,
`parse_pretty_name`, `format_os_version`). That is what lets `/proc/stat`
parsing be tested on Windows and PDH counter interpretation be reasoned about
from Fedora.

The engine itself is tested with `MockProvider`
(`src-tauri/src/metrics/providers/mock.rs`), which is `#[cfg(test)]` and
therefore **never compiled into the shipped binary**. PULSE cannot display a
fabricated temperature, because no code capable of producing one exists outside
the test build.

## Cross-platform rule

> A system-facing feature is not considered complete until its behavior on both
> Windows and Fedora Linux has been designed and, whenever materially testable,
> validated.

For a provider this means: a metric family implemented on one platform must, on
the other, either be implemented or be **declared with an explicit
`unsupported` availability and a reason**. Silence is not an acceptable
cross-platform answer.

## Planned providers

| Provider                    | Platform         | Phase |
| --------------------------- | ---------------- | ----- |
| `linux.cpu`                 | Fedora           | 2     |
| `windows.pdh`               | Windows          | 2     |
| `linux.hwmon`               | Fedora           | 2–3   |
| `nvidia.nvml`               | both             | 3     |
| `linux.drm` (AMD/Intel GPU) | Fedora           | 3     |
| `storage.smart`             | both, privileged | later |

See [`../platforms/fedora.md`](../platforms/fedora.md) and
[`../platforms/windows.md`](../platforms/windows.md) for the data sources and
their constraints.
