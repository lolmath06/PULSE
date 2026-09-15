# Metric Identifiers

> Why identity is modelled the way it is, and the rules for choosing one.

Identifiers are the part of PULSE hardest to change later: they end up inside
saved dashboards, exported presets, alert rules and stored history. A key
renamed in year two silently breaks every configuration that referenced it.
Everything below follows from that.

## Two halves

```text
MetricRef
├── key       cpu.usage.total          what is measured
└── sourceId  cpu:0                    what it is measured on
```

The same key exists many times on one machine:

| Key                     | Sources on one laptop                                                  |
| ----------------------- | ---------------------------------------------------------------------- |
| `gpu.temperature.core`  | `gpu:pci-0000-01-00-0` (RTX 4070), `gpu:pci-0000-00-02-0` (Intel iGPU) |
| `storage.temperature`   | `storage:nvme0n1`, `storage:nvme1n1`                                   |
| `network.download.rate` | `network:enp5s0`, `network:wlp3s0`                                     |

This is exactly why key and source are separate types rather than one string.
Collapsing them forces either a key explosion (`gpu0.temperature.core`,
`gpu1.temperature.core`) or an inability to say "show me core temperature for
_this_ GPU".

## MetricKey

Format: 2 to 6 dot-separated segments; each segment starts with a lowercase
letter and continues with lowercase letters, digits or `_`; 128 characters
maximum.

```text
<domain>.<aspect>[.<qualifier>…]
```

Examples following the convention:

```text
cpu.usage.total
cpu.frequency.current
cpu.temperature.package

memory.used
memory.available

gpu.usage.core
gpu.temperature.core
gpu.temperature.hotspot
gpu.memory.used

storage.temperature
storage.read.rate

network.download.rate
network.upload.rate
```

Rules:

- **Lowercase, always.** Uppercase is rejected rather than normalised, so two
  providers cannot disagree about casing while both appearing to work.
- **Never translated, never user-visible.** The key is machine vocabulary; the
  `displayName` is what the user reads.
- **Never contains the device.** `gpu.temperature.core`, not
  `rtx4070.temperature.core`.
- **Never contains the source of the data.** `cpu.temperature.package`, not
  `hwmon.coretemp.package` — the same key must be satisfiable by `hwmon` on
  Fedora and by a different mechanism on Windows.
- **Stable.** Renaming a key is a breaking change; it needs a schema version
  bump and a migration plan.

Validation lives in `MetricKey::new` and is tested against every example above.

## SourceId

Format: `kind:instance`.

- `kind` — a lowercase word, one of the documented canonical kinds: `system`,
  `cpu`, `gpu`, `memory`, `storage`, `network`, `battery`, `fan`, `power`. The
  list is a convention, not a closed enum, so new hardware classes do not
  require a contract change; `SourceId::has_canonical_kind()` flags divergence.
- `instance` — lowercase letters, digits, `-`, `_`, `.`. **May start with a
  digit**, because real device instances often are numbers (`cpu:0`,
  `storage:2`).

```text
system:host
cpu:0
gpu:pci-0000-01-00-0
storage:nvme0n1
storage:ata-samsung_ssd_870
network:enp5s0
battery:bat0
```

## The instance must be stable — and must not be the product name

> **The human-readable device name must never be the technical identifier.**

Consider a machine with two identical Samsung 990 Pro drives. If identity were
the product name:

- both drives collapse into one identifier;
- a driver update that changes the reported string invalidates every saved
  widget;
- a localised or user-edited name breaks the configuration;
- two machines with the same hardware cannot have different dashboards.

So the instance is derived from something the OS considers stable:

| Class       | Fedora / Linux                                    | Windows                  |
| ----------- | ------------------------------------------------- | ------------------------ |
| GPU         | PCI address from `/sys/class/drm/card*/device`    | PCI location path / LUID |
| Storage     | kernel device name or `/dev/disk/by-id`           | disk number plus serial  |
| Network     | interface name (`enp5s0`), predictable by systemd | interface GUID           |
| CPU package | package index                                     | package index            |

The product name lives in `MetricDefinition.sourceLabel`, which is presentation
only and may change freely between runs.

Where a source identifier is derived from something _not_ guaranteed stable
(hwmon numbering, for instance, can shift between reboots), the provider must
map it to something that is — the `name` attribute plus a label, never
`hwmon2`. This is called out in
[`../platforms/fedora.md`](../platforms/fedora.md).

## ProviderId

Format: dot-separated lowercase segments, 64 characters maximum:
`linux.cpu`, `windows.pdh`, `nvidia.nvml`, `linux.hwmon`.

Appears in `MetricDefinition.providerId` and in engine status, so a failure can
be attributed to a specific provider rather than to "the metrics system".

## Validation

All three types are validated newtypes, not bare strings, and validation runs on
deserialisation too. A malformed identifier arriving from the frontend produces
a clean error at the IPC boundary rather than an unresolvable reference deep
inside the engine.

## Changing an identifier

1. Bump `METRICS_SCHEMA_VERSION`.
2. Document the old and new names in `CHANGELOG.md`.
3. Provide a migration for saved dashboards, or accept that they lose that
   widget and say so.

There is no step that makes this cheap. That is the point of getting the
convention right now.
