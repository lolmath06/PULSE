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
  `cpu`, `gpu`, `memory`, `storage`, `volume`, `network`, `battery`, `fan`,
  `power`. The list is a convention, not a closed enum, so new hardware classes
  do not require a contract change; `SourceId::has_canonical_kind()` flags
  divergence.
- `instance` — lowercase letters, digits, `-`, `_`, `.`. **May start with a
  digit**, because real device instances often are numbers (`cpu:0`,
  `storage:2`).

```text
system:host
cpu:0
gpu:pci-0000-01-00-0
storage:wwid-eui.002538b331b36d03
storage:serial-s677nx0w
volume:wwid-eui.002538b331b36d03-p8
volume:guid-volume-d2b1f8e0-1111-2222-3333-100000000000
network:enp5s0
battery:bat0
```

### `storage:` and `volume:` are two kinds on purpose

`storage:` names a **physical device**; `volume:` names a **filesystem**. They
are related and not interchangeable — a disk holds many filesystems, a
filesystem can span disks, and one filesystem is often reachable at several
mount points at once. Folding them into one kind would let a widget bound to
"the SSD" silently resolve to a filesystem that happens to live on it. See
[`storage.md`](storage.md#a-device-is-not-a-volume-and-a-volume-is-not-a-mount-point).

## Logical sources

Not every source is a device. `cpu:system` and `memory:system` are **logical**
identifiers meaning "this machine's CPU as a whole" and "this machine's physical
memory as a whole".

They are stable by construction: nothing about them can shift with detection
order, driver updates or hardware changes, and they are identical on Fedora and
Windows. A logical source is the honest way to say "the aggregate".

### `cpu:package-N` — the kernel's own package numbering

Phase 5 added one source per processor package:

```text
cpu:package-0
cpu:package-1
```

`N` is the platform's own package index — `physical_package_id` on Linux, which
is exactly what `coretemp` labels its channels with (`Package id 0`) and what
`cpu.count.package` is counted from. PULSE deliberately does **not** create a
second, thermal-only numbering: a machine's package 1 must be package 1
everywhere, or a dual-socket machine eventually attributes one socket's
temperature to the other.

Like `cpu:logical-N`, it is a slot rather than a serial: it identifies "the
package this machine calls number N" and means nothing on another machine.

### `cpu:logical-N` — a slot, not a serial number

Phase 3 added one source per logical processor:

```text
cpu:logical-0
cpu:logical-7
cpu:logical-31
```

These sit in a third category, between a logical aggregate and a
hardware-derived device identity. They identify **a logical slot of this
system**, and the guarantees are correspondingly narrower:

| Guarantee                                 | Holds?                                                               |
| ----------------------------------------- | -------------------------------------------------------------------- |
| Stable across reboots on the same machine | Yes, for as long as the CPU configuration is                         |
| Stable across a PULSE restart             | Yes — the mapping is derived deterministically, not from probe order |
| Meaningful on a **different** machine     | **No**                                                               |
| A claim about the silicon behind it       | **No** — ordinal 7 may be an E-core here and an SMT sibling there    |

Unlike `storage:nvme0n1` or `gpu:pci-0000-01-00-0`, which are derived from
device identity, a logical processor ordinal is derived from _position_. A
dashboard built on a 32-thread laptop and opened on a 4-thread virtual machine
will find `cpu:logical-7` simply absent.

On Linux the ordinal is the kernel's own CPU number (`cpu7` → `cpu:logical-7`),
so it matches `htop` and `taskset`. On Windows it is assigned by sorting
`(processor group, index in group)` — a machine over 64 logical processors has
no flat native number to borrow. Both are documented in
[`cpu-advanced.md`](cpu-advanced.md).

### GPU sources

Phase 4 added one source per graphics adapter, and GPUs make the identity
problem sharper than anything before them — every obvious candidate is an
enumeration artefact:

| Candidate              | Why it is not an identity                                                                           |
| ---------------------- | --------------------------------------------------------------------------------------------------- |
| `/sys/class/drm/card0` | A DRM minor number, assigned in probe order; `card0` and `card1` can swap between boots             |
| NVML index             | NVIDIA documents it as unstable across reboots, and it shifts when a GPU is added, removed or reset |
| DXGI adapter index     | Enumeration order, reflecting which adapter Windows currently prefers                               |
| `AdapterLuid`          | Microsoft documents it as valid **only until the system restarts**                                  |
| Product name           | Two identical cards collapse into one identifier                                                    |

All three indices are used during enumeration and **none is ever stored**. What
PULSE stores instead, in descending order of strength:

```text
gpu:nvidia-11111111-2222-3333-4444-555555555555   NVML UUID   (hardware)
gpu:pci-0000-01-00-0                              PCI address (slot)
gpu:amd-73ff-10020e3b-c1                          device model
gpu:amd-73ff-10020e3b-c1-n1                       device model, session-disambiguated
```

The **stability level is recorded in the descriptor** rather than assumed,
because the honest answer differs by platform and vendor. An NVML UUID survives
anything; a PCI address survives everything except moving the card to another
slot; a DXGI device-model tuple survives reboots but tells two identical cards
apart only within one session. PULSE does not claim more than the platform
gives. See [`gpu.md`](gpu.md).

**The dashboard implication.** A saved dashboard must eventually be able to
express _"every `cpu.usage.logical`"_ as one selection rather than as
thirty-two individual references — otherwise it breaks on any machine with a
different processor count, and misses processors added by hotplug. The
identifier scheme was chosen to make that possible: every per-processor metric
shares one key and differs only in source, which is exactly the shape a
key-wide selector needs. That selector is a later phase; only the identifiers
are settled here.

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

| Class       | Fedora / Linux                                   | Windows                            |
| ----------- | ------------------------------------------------ | ---------------------------------- |
| GPU         | PCI BDF from `/sys/class/drm/card*/device`       | PCI location path / LUID           |
| Storage     | `wwid` (WWN/NGUID/EUI/T10), else serial          | serial, else device instance ID    |
| Volume      | parent device plus partition, else `major:minor` | volume GUID path                   |
| Network     | MAC, or a systemd-predictable name               | interface GUID                     |
| CPU package | package index                                    | package index                      |
| Logical CPU | kernel CPU number                                | sorted `(group, index)` ordinal    |
| GPU         | NVML UUID, else PCI address                      | NVML UUID, else device-model tuple |

A provider that can only obtain an enumeration name must map it to something
stable before publishing it — hwmon, for instance, must be keyed on the `name`
attribute plus a label, never on `hwmon2`. Where nothing stable exists, that
limitation belongs in the platform documentation, not hidden inside an
identifier that will silently break a user's dashboard.

Since Phase 6 the storage descriptors go one step further and **record which
mechanism was used**: `IdentityStability` distinguishes `Hardware`,
`SystemAssigned`, `DerivedFromParent` and `Session`, so a weak identity is
inspectable rather than assumed. A `storage:dev-…` or `volume:mm-…` source is
session-scoped by construction, and the prefix says so.

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
