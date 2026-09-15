# Metrics Model

> Phase 1. The model and the engine exist and are tested; **no real hardware
> metric is collected yet**.

Every type here is defined in `src-tauri/src/metrics/model/` and mirrored in
`src/types/metrics.ts`.

## The four concepts

The model keeps apart four things that are easy to conflate, and most of its
value comes from that separation.

| Concept                   | Type               | Answers                             |
| ------------------------- | ------------------ | ----------------------------------- |
| What is measured          | `MetricKey`        | "GPU core temperature"              |
| What it is measured on    | `SourceId`         | "the discrete GPU at PCI 01:00.0"   |
| What PULSE knows about it | `MetricDefinition` | unit, category, kind, availability  |
| What it read just now     | `MetricSample`     | a value and a timestamp, or why not |

Identity (`MetricKey` + `SourceId`) is stable and stored in dashboards.
Presentation (`sourceLabel`, `displayName`, `description`) is free to change.
See [`identifiers.md`](identifiers.md).

## MetricRef

```jsonc
{ "key": "gpu.temperature.core", "sourceId": "gpu:pci-0000-01-00-0" }
```

The full identity of one concrete measurement. This pair — and only this pair —
is what a saved dashboard stores.

## MetricDefinition

```jsonc
{
  "metric": { "key": "gpu.temperature.core", "sourceId": "gpu:pci-0000-01-00-0" },
  "sourceLabel": "NVIDIA GeForce RTX 4070 Laptop GPU",
  "displayName": "Core temperature",
  "description": "Temperature of the GPU core die.",
  "category": "gpu",
  "unit": "celsius",
  "valueType": "number",
  "kind": "gauge",
  "availability": { "status": "available" },
  "providerId": "nvidia.nvml",
}
```

A metric that cannot currently be read **still appears in the catalog**, with a
non-available `availability`. That is what lets the UI say _"your board exposes
no fan sensor"_ instead of silently omitting the metric and leaving the user
wondering.

## MetricSample

```jsonc
{
  "metric": { "key": "cpu.usage.total", "sourceId": "cpu:0" },
  "timestamp": 1758000000000,
  "value": { "type": "number", "value": 37.5 },
  "availability": { "status": "available" },
}
```

`value` is `null` whenever `availability.status` is not `available`.

**Timestamps are Unix epoch milliseconds** (`u64`), chosen because that is
exactly what JavaScript's `Date` and every charting library expect — no
conversion on the frontend. A system clock set before 1970 yields `0` rather
than a panic.

## MetricValue

A discriminated union, so TypeScript needs no runtime type sniffing:

```jsonc
{ "type": "number",  "value": 42.5 }
{ "type": "boolean", "value": true }
{ "type": "text",    "value": "connected" }
```

**Non-finite numbers are rejected at construction.** `serde_json` serialises
`NaN` and infinity as JSON `null`, which would arrive on the frontend as a
`number` value that is not a number, silently violating the contract. A
non-finite reading is a failed reading: `MetricSample::number` turns it into a
`providerError` with code `parse`.

## Units — the canonical rule

> **The backend always reports values in the canonical unit. The frontend
> converts for display.**

| Unit                      | Canonical form                     |
| ------------------------- | ---------------------------------- |
| `percent`                 | 0–100                              |
| `ratio`                   | 0–1                                |
| `celsius`                 | degrees Celsius                    |
| `hertz`                   | hertz — not kHz, MHz or GHz        |
| `bytes`                   | bytes — not KB, MB or GB           |
| `bytesPerSecond`          | bytes per second                   |
| `watts`, `volts`, `rpm`   | watts, volts, RPM                  |
| `milliseconds`, `seconds` | as named                           |
| `count`                   | a dimensionless count              |
| `none`                    | no unit (boolean and text metrics) |

The UI may render `3.8 GHz`, `15.4 GB` or `°F`; the number crossing the IPC
boundary never changes. This keeps stored history, alert thresholds and
dashboard configuration comparable across machines and across PULSE versions.

`percent` and `ratio` are deliberately distinct units. Conflating them is a
factor-of-100 bug that looks plausible in a chart.

**Durations:** use `milliseconds` for latencies and short intervals (ping, frame
time), `seconds` for long ones (uptime, battery time remaining). Pick whichever
avoids absurd magnitudes and state it in the metric's description.

## MetricKind

| Kind      | Meaning                                    | Aggregation               |
| --------- | ------------------------------------------ | ------------------------- |
| `gauge`   | Instantaneous reading that rises and falls | May be averaged           |
| `counter` | Monotonically increasing total             | Must be differenced first |
| `state`   | Discrete condition                         | Counted, not averaged     |

Examples: CPU usage and temperature are gauges; total bytes downloaded is a
counter; "internet connected" is a state.

This is declared rather than inferred because getting it wrong produces charts
that look plausible and are wrong.

## Categories

`system`, `cpu`, `gpu`, `memory`, `storage`, `network`, `sensors`, `fan`,
`power`, `battery`, `process`, `other`.

A category is a **user-facing grouping** for navigation and widget pickers, not
a taxonomy of hardware. Resist adding one per sensor chip.

## Availability — why it has seven variants

This is one of the most important types in PULSE. The differences matter
enormously to the user:

| Status                   | Means                                                 | User can                       |
| ------------------------ | ----------------------------------------------------- | ------------------------------ |
| `available`              | Readable now                                          | —                              |
| `unsupported`            | No implementation on this platform or in this build   | Nothing; our problem           |
| `notDetected`            | Capability exists, this machine has no such component | Nothing; expected              |
| `permissionDenied`       | Blocked by OS permissions                             | Grant elevation                |
| `temporarilyUnavailable` | Normally works, not right now                         | Wait                           |
| `providerError`          | The provider failed                                   | Depends on `error.recoverable` |
| `notRegistered`          | The reference is not in the catalog                   | Fix the dashboard              |

Collapsing these into a generic error — or worse, into a silent `0` — is the
single most common failure of system monitors. PULSE's contract refuses to.

`notRegistered` is the one addition to the six hardware-facing states: it
describes the _request_ rather than the hardware, and exists so that one stale
dashboard entry cannot fail a whole sampling call.

## MetricError

```jsonc
{ "code": "permissionDenied", "message": "needs root", "recoverable": false }
```

`code` is for code; `message` is for humans. **The frontend never parses the
message.** Codes: `unknownMetric`, `duplicateMetric`, `unsupported`,
`notDetected`, `permissionDenied`, `io`, `parse`, `timeout`,
`providerUnavailable`, `missingSample`, `internal`.

`recoverable` defaults from the code (`io`, `timeout`, `providerUnavailable`,
`missingSample` and `notDetected` are retryable) and can be overridden when the
provider knows better.

## Contract version

`METRICS_SCHEMA_VERSION = 1`, defined in `src-tauri/src/metrics/mod.rs` and
mirrored in `src/types/metrics.ts`.

Bump it for an **incompatible** change: a removed or renamed field, a changed
unit convention, a repurposed enum variant. Adding an optional field does not
require a bump. The frontend compares versions and warns rather than
misinterpreting payloads. There is deliberately no migration machinery yet —
detecting the mismatch is what matters at this stage.

## Keeping Rust and TypeScript in sync

The two definitions are duplicated by hand, and that duplication is guarded
rather than trusted:

- every payload type has a serialisation test asserting **camelCase** field
  names and exact enum discriminants;
- `MetricDefinition`, `MetricSample` and `EngineStatus` additionally assert
  their **exact field set**, so adding a Rust field without updating
  `src/types/metrics.ts` fails `cargo test`;
- `METRICS_SCHEMA_VERSION` is asserted on both sides.

Code generation (`ts-rs`, `specta`) was considered and rejected for this phase:
it adds a build-time dependency and a generated-file workflow to solve a problem
that four tests already catch. Worth revisiting when the model stops changing.
