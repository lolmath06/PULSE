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

## Logical sources

Not every source is a device. `cpu:system` and `memory:system` — the sources
PULSE ships today — are **logical** identifiers meaning "this machine's CPU as a
whole" and "this machine's physical memory as a whole".

They are stable by construction: nothing about them can shift with detection
order, driver updates or hardware changes, and they are identical on Fedora and
Windows. Per-core, per-package and per-DIMM sources will arrive later with their
own hardware-derived identifiers; until then, a logical source is the honest way
to say "the aggregate".

## The instance must be stable — and must not be the product name

> **The human-readable device name must never be the technical identifier.**

Consider a machine with two identical Samsung 990 Pro drives. If identity were
the product name:

- both drives collapse into one identifier;
- a driver update that changes the reported string invalidates every saved
  widget;
- a localised or user-edited name breaks the configuration;
- two machines with the same hardware cannot have different dashboards.

The product name lives in `MetricDefinition.sourceLabel`, which is presentation
only and may change freely between runs.

### Kernel enumeration names are not identities either

This deserves stating plainly, because it is the trap that will bite the GPU and
storage phases:

> **`nvme0n1`, `card0`, `eth0` and `hwmon2` are enumeration artefacts, not
> identities.** They depend on probe order, and can change between boots, after
> a kernel or firmware update, or when a device is added or removed.

Preferred sources of a stable instance, in rough order:

```text
PCI BDF address          e.g. 0000:01:00.0
hardware serial number
UUID / WWN
/dev/disk/by-id entry
Windows device instance ID
Windows interface GUID
```

Applied per class:

| Class       | Fedora / Linux                             | Windows                        |
| ----------- | ------------------------------------------ | ------------------------------ |
| GPU         | PCI BDF from `/sys/class/drm/card*/device` | PCI location path / LUID       |
| Storage     | `/dev/disk/by-id`, serial or WWN           | device instance ID plus serial |
| Network     | MAC, or a systemd-predictable name         | interface GUID                 |
| CPU package | package index                              | package index                  |

A provider that can only obtain an enumeration name must map it to something
stable before publishing it — hwmon, for instance, must be keyed on the `name`
attribute plus a label, never on `hwmon2`. Where nothing stable exists, that
limitation belongs in the platform documentation, not hidden inside an
identifier that will silently break a user's dashboard.

This is expanded in [`../platforms/fedora.md`](../platforms/fedora.md).

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
