# PULSE — Architecture Overview

> Status: Phase 0 (foundation). This document describes the shape of the system
> and the rules it must keep, not a finished implementation.

## 1. The project rule

PULSE targets **Windows and Fedora Linux as two first-class platforms**. Neither
is the reference platform; neither is experimental. From that follows the rule
every contributor is held to:

> **A system-facing feature is not considered complete until its behavior on
> both Windows and Fedora Linux has been designed and, whenever materially
> testable, validated.**

"Designed" means the platform-specific data source, its failure modes and its
permission requirements are written down. "Validated" means the behaviour was
observed on a real machine, or — when no such machine is available — exercised
by CI and explicitly flagged as awaiting a physical test.

## 2. Layers

```text
┌─────────────────────────────────────────────┐
│ React UI                                    │  src/
│ pages, layouts, widgets, stores             │
└───────────────────┬─────────────────────────┘
                    │ src/services/ — the only callers of invoke()
┌───────────────────▼─────────────────────────┐
│ Tauri command surface                       │  src-tauri/src/commands/
│ thin, serialisable, no platform branching   │
└───────────────────┬─────────────────────────┘
┌───────────────────▼─────────────────────────┐
│ Application core (cross-platform)           │  src-tauri/src/services/
│ services, metrics engine, state             │  src-tauri/src/metrics/
│                                             │  src-tauri/src/state/
│   MetricsEngine                             │
│     ├── Provider A ──┐                      │
│     ├── Provider B ──┼── the only code that │
│     └── Provider C ──┘   touches hardware   │
└───────────────────┬─────────────────────────┘
┌───────────────────▼─────────────────────────┐
│ Platform abstraction                        │  src-tauri/src/platform/
│   ├── linux/    /proc, /sys, hwmon, Wayland │
│   └── windows/  WMI, PDH, vendor SDKs       │
└─────────────────────────────────────────────┘
```

### Non-negotiable boundaries

1. **The frontend never touches the system.** No `/proc`, `/sys`, `hwmon`, WMI,
   PDH, NVML or vendor SDK is ever reachable from React. Everything crosses the
   boundary as a structured, serialisable payload.
2. **Only `platform/` knows which OS it is on.** `#[cfg(target_os = "...")]` for
   the purpose of OS selection lives in `platform/mod.rs` and its submodules.
   Services and commands depend on the `HostPlatform` trait, never on a `cfg`.
3. **A platform-only dependency is declared per target.** In `Cargo.toml`,
   Windows crates sit under `[target.'cfg(target_os = "windows")'.dependencies]`
   so a Windows dependency can never break a Fedora build, and vice versa.
4. **Commands stay thin.** They validate, delegate to a service, and return.
   Logic in a command is logic that cannot be unit-tested.

### The platform seam

```rust
pub trait HostPlatform: Send + Sync {
    fn kind(&self) -> PlatformKind;
    fn os_version(&self) -> Option<String>;
    fn display_server(&self) -> Option<String> { None }
}
```

`platform::host()` is the single function that selects an implementation.
Adding a capability is always the same three steps:

1. add a method to `HostPlatform` (with a safe default where one exists);
2. implement it in `linux/` **and** `windows/`;
3. expose it through a service and a command.

A third variant, `UnsupportedPlatform`, keeps the crate compiling and testable
on platforms PULSE has no integration for (macOS, BSD) instead of failing the
build. `PlatformKind::is_supported()` lets callers react to it.

## 3. Frontend structure

```text
src/
├── app/          # composition root: router, routes, app-wide constants
├── components/   # reusable presentational components
├── features/     # self-contained feature slices (UI + state + logic)
├── hooks/        # reusable React hooks
├── layouts/      # page frames (shell, future overlay frame)
├── pages/        # one component per route
├── services/     # the invoke() boundary — the only place calling Tauri
├── stores/       # client state (dashboard layouts, preferences)
├── styles/       # design tokens and global CSS
├── types/        # shared types, including mirrors of Rust payloads
└── utils/        # pure helpers
```

Folders are created as they earn their content; the architecture is fixed even
where a folder is still empty.

Two conventions matter already:

- **`services/` is the only place that calls `invoke`.** Components and hooks
  call services. This keeps the IPC surface enumerable and mockable.
- **Rust payloads are mirrored in `types/`.** `PlatformInfo` exists in
  `src/types/platform.ts` and `src-tauri/src/platform/mod.rs`; Rust serialises
  with `#[serde(rename_all = "camelCase")]` so the two read identically. A Rust
  test asserts the casing so the mirror cannot silently drift.

## 4. The Metrics Engine

**Implemented.** Phase 1 built the model, the provider contract and the engine;
Phase 2 added the first native collectors — CPU usage and physical memory, on
both Fedora and Windows; Phase 3 grew the CPU support to per-logical-processor
usage and frequency plus topology; Phase 4 added GPU inventory, identity and
core telemetry across NVIDIA, AMD and Intel; Phase 5 added temperatures and fan
speeds to both families. The engine did not change to accommodate any of them,
which was the point of building it first — including when the catalog stopped
being a fixed list and became `9 + 3N + P + 11G` entries sized by the host's
processors, packages and adapters.

The engine is the second boundary in PULSE, after the platform layer, and it
exists to make one sentence true:

> All future PULSE metrics can be added behind a single contract without the
> interface needing to know whether they come from Fedora, Windows, NVIDIA,
> `/proc`, WMI or SMART.

Four separations carry that weight:

- **Identity vs presentation.** `MetricKey` ("GPU core temperature") and
  `SourceId` ("the GPU at PCI 01:00.0") are stable identifiers stored in
  dashboards; `displayName` and `sourceLabel` are free to change. A device's
  product name is never an identifier.
- **Definition vs sample.** What PULSE knows about a metric — unit, category,
  kind, availability — is separate from what it read a moment ago.
- **Canonical vs display units.** The backend reports hertz, bytes and Celsius;
  the frontend renders GHz, GB and °F. Stored history and alert thresholds stay
  comparable.
- **Availability as a first-class state.** Seven distinct states keep "no such
  sensor on this machine" apart from "needs elevation", "the driver hiccupped"
  and "not implemented on this platform". Collapsing them — or showing `0` — is
  the most common failure of system monitors, and the contract refuses to.

The engine owns the catalog and routes references to providers via a `HashMap`
index, samples only what is requested, and isolates provider failures: a GPU
provider going down does not blank out CPU and network readings.

### Where providers come from

The engine hosts providers; it never discovers them.

```text
HostPlatform::metric_providers()     the platform layer decides what exists
        │  Vec<Arc<dyn MetricProvider>>
services::metrics::build_engine()    the composition point
        │
metrics::build_engine(providers)     registers each, logs and skips failures
        │
MetricsEngine                        contains no cfg(target_os) at all
```

`services` sits above both `metrics` and `platform`, so neither depends on the
other. This is what keeps OS branching out of the engine entirely: `grep
cfg(target_os)` over `src-tauri/src/metrics/` matches nothing but documentation
examples.

Shared metric declarations live in `metrics/wellknown/`, which owns each
metric's key, unit, kind, user-facing text and arithmetic. Platform providers
supply only raw counters and the topology they discovered. For metric families
whose size depends on the machine, `wellknown` exposes a **generator** —
`cpu::definitions(provider, topology)`, `gpu::definitions(provider, devices)` —
rather than a constant list, so neither platform invents its own per-processor
or per-device keys and labels.

A provider owns a whole metric _family_, not a device: `linux.gpu` hosts the DRM
inventory, the NVML capability and the AMDGPU capability together. Splitting
them would make two providers claim the same `MetricRef` for a card both can
see, which the engine rejects by design.

Contract tests assert that the Linux and Windows declarations, generated for the
same synthetic topology, differ in `providerId` and nothing else — that is the
mechanism behind "a dashboard configured on Fedora still works on Windows".
Availability is the one field each platform decides for itself, because the same
machine may genuinely expose a frequency on one OS and not the other.

**Deliberately deferred**: the scheduler, subscriptions, streaming events,
history and its ring buffer. The interaction model is still
`request → sample → response`, with the UI refreshing on demand.

See [`../metrics/README.md`](../metrics/README.md),
[`../metrics/model.md`](../metrics/model.md),
[`../metrics/identifiers.md`](../metrics/identifiers.md) and
[`../metrics/providers.md`](../metrics/providers.md).

## 5. Widgets (future)

A widget is a pure rendering of one or more metric subscriptions plus a
serialisable configuration. It knows nothing about the OS, and — since Phase 1 —
nothing about where its numbers come from either: it holds a `MetricRef` and
reads `MetricDefinition` and `MetricSample`. This is what makes a widget
portable across Windows and Fedora for free; the platform differences were
already resolved two layers below it.

See [`../widgets/README.md`](../widgets/README.md).

## 6. Modes vs dashboards

These are **two orthogonal concepts**, and conflating them is the single
easiest way to make the Mini overlay impossible to build later.

### Modes — _how_ PULSE behaves

A mode describes the window, the interaction model and the resource budget.

| Mode            | Window                                         | Interaction                 | Budget                                   |
| --------------- | ---------------------------------------------- | --------------------------- | ---------------------------------------- |
| **Standard**    | Normal decorated window                        | Full navigation and editing | Normal                                   |
| **Mini**        | Borderless, transparent, always-on-top overlay | Minimal or click-through    | Deliberately low                         |
| **Gaming**      | Standard window or Mini overlay                | Read-mostly                 | Low: few metrics, slow cadence           |
| **Development** | Standard window                                | Full                        | Normal, biased to build-pressure metrics |

Modes are enumerated in `src/types/mode.ts`.

### Dashboards — _what_ is displayed

A dashboard is a named, serialisable set of widgets and their layout. Several
dashboards can exist for different uses, and the Personal dashboard is meant to
be extremely free: arbitrary placement, resizing, and 20+ widgets.

### Why the separation matters

- The same dashboard can be rendered in Standard and in Gaming mode.
- Mini is _not_ "a dashboard that is small" — it is a different window mode that
  happens to render a very constrained dashboard.
- A user can switch mode without losing dashboard configuration, and switch
  dashboard without changing window behaviour.

Neither the mode engine nor the dashboard engine is implemented in Phase 0.
Phase 0 only fixes the vocabulary and wires navigation.

## 7. Where the Mini overlay fits

The Mini overlay is the strongest constraint on the architecture, because it
requires the backend to keep producing data while the main UI is not visible.
That is why history and sampling live in Rust rather than in the webview, and
why the overlay is planned as a **separate Tauri window** rather than a CSS
state of the main window.

See [`mini-overlay.md`](mini-overlay.md).
