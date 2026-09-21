# Storage Metrics

How PULSE inventories disks and filesystems, measures their activity, and
reports what an NVMe controller says about its own health — and, as always,
which numbers it refuses to publish.

> **PULSE observes. It does not control.**
> Nothing here mounts, unmounts, partitions, formats, trims, repairs or writes
> to a storage device. Every device handle is opened read-only, and the only
> NVMe command PULSE can issue is `Get Log Page`. See
> [Read-only, without exception](#read-only-without-exception).

---

## A device is not a volume, and a volume is not a mount point

Three words that monitoring tools routinely use interchangeably, and that mean
three different things:

| Concept         | What it is                                     | Example                             |
| --------------- | ---------------------------------------------- | ----------------------------------- |
| **Device**      | A physical piece of hardware that stores bytes | the Samsung SSD in the M.2 slot     |
| **Volume**      | A filesystem living on some of that hardware   | the btrfs filesystem holding Fedora |
| **Mount point** | A path where a volume is currently reachable   | `/`, `/home`, `C:\`                 |

Their cardinalities do not line up:

- one device holds **many** volumes;
- one volume can span **several** devices (a striped or spanned volume);
- one volume is frequently reachable at **several** mount points at once —
  Fedora's default layout mounts the _same_ btrfs filesystem at both `/` and
  `/home`, and a bind mount adds a third path to a filesystem already listed.

Collapsing any pair of these produces a monitor that double-counts capacity and
attributes writes to the wrong disk. PULSE therefore models devices and volumes
as **two source kinds**, and treats a mount point as _presentation_ attached to
a volume rather than as an identity:

```text
storage:<instance>     a physical device
volume:<instance>      a filesystem
```

`SourceId::has_canonical_kind` knows both, and a contract test asserts that no
per-device metric ever lands on a `volume:` source or the reverse.

---

## The metrics

For `D` physical devices and `V` volumes the catalog holds
`2 + 13D + 4V` storage metrics. Nothing hardcodes `D` or `V`; the tests derive
the expected size from the catalog.

### Machine-wide — `storage:system`

| Metric                 | Unit  | Kind  | Value  |
| ---------------------- | ----- | ----- | ------ |
| `storage.device.count` | count | state | number |
| `storage.volume.count` | count | state | number |

Both are `state` rather than `gauge` for the same reason as `cpu.count.*` and
`gpu.count`: they are discrete facts about the machine, and a `state` cannot be
averaged, so history can never produce "1.4 disks".

### Per device — the device's own source

| Metric                              | Unit                | Kind    | Value  |
| ----------------------------------- | ------------------- | ------- | ------ |
| `storage.capacity.total`            | bytes               | state   | number |
| `storage.io.read.bytes_per_second`  | bytesPerSecond      | gauge   | number |
| `storage.io.write.bytes_per_second` | bytesPerSecond      | gauge   | number |
| `storage.io.read.iops`              | operationsPerSecond | gauge   | number |
| `storage.io.write.iops`             | operationsPerSecond | gauge   | number |
| `storage.io.read.latency`           | milliseconds        | gauge   | number |
| `storage.io.write.latency`          | milliseconds        | gauge   | number |
| `storage.health.temperature`        | celsius             | gauge   | number |
| `storage.health.percentage_used`    | percent             | gauge   | number |
| `storage.health.available_spare`    | percent             | gauge   | number |
| `storage.health.power_on_hours`     | hours               | counter | number |
| `storage.health.unsafe_shutdowns`   | count               | counter | number |
| `storage.health.media_errors`       | count               | counter | number |

### Per volume — the volume's own source

| Metric                              | Unit    | Kind  | Value  |
| ----------------------------------- | ------- | ----- | ------ |
| `storage.volume.capacity.total`     | bytes   | state | number |
| `storage.volume.capacity.used`      | bytes   | gauge | number |
| `storage.volume.capacity.available` | bytes   | gauge | number |
| `storage.volume.usage.percent`      | percent | gauge | number |

### Where each one comes from

| Metric group    | Fedora                                           | Windows                                                        |
| --------------- | ------------------------------------------------ | -------------------------------------------------------------- |
| Inventory       | `/sys/class/block`                               | SetupAPI + `IOCTL_STORAGE_QUERY_PROPERTY`                      |
| Capacity        | `/sys/block/<dev>/size`                          | `IOCTL_DISK_GET_DRIVE_GEOMETRY_EX`                             |
| I/O counters    | `/proc/diskstats`                                | `IOCTL_DISK_PERFORMANCE`                                       |
| Volumes         | `/proc/self/mountinfo`                           | `FindFirstVolumeW` / `GetVolumePathNamesForVolumeNameW`        |
| Volume usage    | `statvfs(3)`                                     | `GetDiskFreeSpaceExW`                                          |
| Volume → device | the mount's device number, via `/proc/diskstats` | `IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS`                         |
| NVMe health     | `nvme` `hwmon` + `NVME_IOCTL_ADMIN_CMD`          | `IOCTL_STORAGE_QUERY_PROPERTY` + `ProtocolTypeNvme` log `0x02` |

---

## Two new units

`storage.io.*` forced two additions to the shared contract, and neither is
cosmetic:

| Unit                  | Why it is not an existing one                                                                                                                       |
| --------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------- |
| `operationsPerSecond` | A _rate_ of operations is not a `count` of them. Only one depends on the interval it was measured over                                              |
| `hours`               | An NVMe controller counts power-on time in whole hours. Converting to `seconds` would invent five orders of magnitude of precision it does not have |

Throughput reuses the existing `bytesPerSecond`, and latency the existing
`milliseconds`. What is _never_ done is publishing a rate as `bytes` or
`count` — a contract test asserts that every `storage.io.*` metric carries a
rate unit, because reusing a quantity unit would let a chart average a
throughput with a total.

---

## Activity is measured, not read

Throughput, IOPS and latency are **rates**. Both operating systems expose
monotonic totals — bytes transferred and operations completed since boot — so a
single absolute read says nothing. The measurement exists only _between_ two
samples:

```text
delta bytes      / elapsed seconds  →  bytes per second
delta operations / elapsed seconds  →  IOPS
delta service time / delta completed operations → mean latency
```

The arithmetic lives once, in `metrics::wellknown::storage::io`, and is shared
by both platforms. The platform backends supply six totals per device and
nothing else.

### Zero is sometimes a measurement and sometimes a lie

This is the distinction the whole module is built around:

| Metric     | Idle device over a real interval                                                          |
| ---------- | ----------------------------------------------------------------------------------------- |
| Throughput | `0 B/s` — **a true measurement.** Nothing was transferred                                 |
| IOPS       | `0 IOPS` — **a true measurement.** No operation completed                                 |
| Latency    | **No value.** Latency is a mean over completed operations, and there were none to average |

Publishing `0 ms` for an idle disk would claim it responds instantly. PULSE
publishes no value and says why.

### The first sample has no baseline

A device's first reading has nothing to difference against. It reports
`temporarilyUnavailable` with "disk activity is measured between two samples;
waiting for the next one" — never `0 B/s`, which a user cannot distinguish from
a genuinely idle disk. The provider primes a baseline at startup so the _second_
request already has a real interval, and the interface shows the waiting state
honestly rather than firing a hidden extra sample to paper over it.

### Counters that go backwards

A counter reset, a suspend/resume cycle, a device unplugged and reconnected, or
Windows's 32-bit operation counters wrapping all make a total go _down_. Any
single field regressing invalidates the whole snapshot as a baseline — a reset
does not pick and choose which totals it clears, and differencing the fields
that happen to still be ascending would publish a rate derived from two
different eras of the device's life. PULSE restarts the baseline and reports
`temporarilyUnavailable` for that one interval.

### Sectors are 512 bytes, always

`/sys/block/<dev>/size` and the `/proc/diskstats` sector counters are expressed
in **fixed 512-byte units**, whatever the device's logical block size. The
tempting formula

```text
size × queue/logical_block_size      // WRONG
```

is correct only by accident on the 512-byte-logical drives that make up most
hardware, and reports **eight times** the real figure on a 4Kn drive — a 2 TB
SSD shown as 16 TB, and a 125 MiB/s read shown as 1 GiB/s. Explicit tests in
`sysfs` and `diskstats` pin the correct constant and assert the wrong formula
gives a different answer.

### What a latency actually means

`storage.io.*.latency` is the **operating system's** view: the wall-clock time
requests spent in flight, queueing included, divided by the number that
completed. It is _not_ the device's internal media latency, and a queue-depth
spike raises it without anything being wrong with the drive. The metric's own
description says so.

---

## `used`, `free` and `available`

Unix filesystems reserve a slice of their blocks for the superuser, so a full
disk still leaves root enough room to fix it. `statvfs` therefore reports two
different notions of emptiness:

```text
f_bfree    blocks that hold no data at all
f_bavail   blocks an unprivileged process may actually allocate
```

PULSE publishes them as:

```text
used      = total - free        space that holds data
available = user-available      space this user can really write to
```

so **`used + available ≤ total`**, with the superuser reserve as the
difference. This is deliberate and matches what `df` reports for each figure: a
user comparing PULSE against `df` on the same filesystem sees the same bytes.
Computing `used = total - available` instead — the other obvious choice — would
report the reserve as consumed, showing several gigabytes of phantom usage on
every ext4 volume.

Windows has no such reserve, so `GetDiskFreeSpaceExW`'s "free" and "available to
caller" coincide and the same formula gives the same answer. **The meaning of
each key is identical on both platforms**; only the gap between them differs,
and on Windows it is normally zero. Under a disk quota it is not, and
`available` then correctly reports what the user can actually write.

> **One difference from `df` worth knowing.** PULSE computes
> `storage.volume.usage.percent` as `used / total`, while `df` prints
> `used / (used + available)` — which is why a filesystem PULSE shows at 58 %
> can read as 60 % in `df`. Every byte figure matches exactly; only the
> percentage convention differs. `used / total` is the one the card's usage bar
> draws, and the one that answers "how full is this filesystem".

---

## Identity

Every obvious candidate is wrong, exactly as it was for GPUs:

| Candidate            | Why it must not be an identity                                                              |
| -------------------- | ------------------------------------------------------------------------------------------- |
| `sda`, `nvme0n1`     | Kernel names assigned in probe order; plugging in a USB stick at boot renames the next disk |
| `\\.\PhysicalDrive0` | A Windows enumeration index, not a property of the hardware                                 |
| `C:`, `/`, `/home`   | Mount points. They change without the filesystem changing                                   |
| Product name         | Two identical SSDs collapse into one identifier                                             |
| `major:minor`        | Stable only while the device stays attached                                                 |

### What PULSE uses instead

**Devices**, in order of preference:

| Mechanism   | Source                                           | Stability      |
| ----------- | ------------------------------------------------ | -------------- |
| `wwid`      | `/sys/.../wwid` — a WWN, NGUID, EUI-64 or T10 ID | Hardware       |
| `serial`    | the device's serial number                       | Hardware       |
| `system-id` | a Windows device instance ID                     | SystemAssigned |
| `os-name`   | `nvme0n1`, `PhysicalDrive0`                      | **Session**    |

**Volumes**, in order of preference:

| Mechanism             | Source                                                | Stability         |
| --------------------- | ----------------------------------------------------- | ----------------- |
| `volume-guid`         | a Windows volume GUID path                            | SystemAssigned    |
| `partition-of-device` | the parent device's instance plus a partition ordinal | DerivedFromParent |
| `device-number`       | the filesystem's `major:minor`                        | **Session**       |

The stability is **recorded, not assumed** — `IdentityStability` is carried on
every descriptor, so a user can be told when a saved widget rests on something
weaker than a hardware identifier rather than finding out after a reboot.

### The same drive gets the same identifier on both platforms

A drive's serial number is the _same string_ whether Fedora read it from
`/sys/block/<dev>/device/serial` or Windows from the storage device descriptor.
Both platforms therefore derive `storage:serial-s677nx0w` for the same physical
drive, and a dashboard built on one opens on the other. A contract test asserts
it for a synthetic drive, driving both platforms' identity code.

Windows's device instance ID is a Windows construct with no Linux counterpart,
which is exactly why it ranks _below_ the serial rather than above it.

### Normalising a real identifier

`SourceId` instances allow lowercase letters, digits, `-`, `_` and `.`. Real
identifiers are messier. The USB disk on the development machine reports

```text
t10.Intenso SCSI            2019131398AB5\0\0\0
```

— padding, and the literal `\0` escapes the kernel writes for the trailing NUL
bytes. The Linux backend strips the NUL escapes (they are padding, and their
number is a property of the bridge rather than of the drive), and the shared
normaliser collapses every remaining disallowed run to a single `-`, yielding

```text
storage:wwid-t10.intenso-scsi-2019131398ab5
```

A string that reduces to nothing is **refused** rather than producing
`storage:wwid-`, so a device reporting all-padding falls through to the next
candidate instead of colliding with every other such device.

### USB placeholder serials

Many USB bridges report a hardcoded serial — every unit of a given enclosure
model shipping with `0123456789ABCDEF` or `000000000000`. Two different disks in
two identical enclosures would then collapse onto one identity, so a widget
bound to one would silently show the other. The Windows identity layer refuses a
serial that is one repeated character or a known placeholder, and falls through
to the device instance ID, which encodes the port and does tell them apart.

---

## What counts as a device

`/sys/class/block` lists every block device the kernel knows, which is a much
larger set than "disks this machine has":

| Entry         | Counted? | Why                                                      |
| ------------- | -------- | -------------------------------------------------------- |
| `nvme0n1`     | yes      | a disk                                                   |
| `nvme0n1p8`   | no       | a partition of that disk, not a device of its own        |
| `sda`         | yes      | a disk, even behind a USB bridge                         |
| `vda`, `xvda` | yes      | a paravirtualised disk — often a VM's _only_ storage     |
| `loop0`       | no       | a file presented as a block device                       |
| `zram0`       | no       | compressed RAM. Real, useful, and not storage            |
| `dm-0`        | no       | a device-mapper view of storage already counted below it |
| `md0`         | no       | likewise for software RAID                               |
| `sr0`, `fd0`  | no       | optical and floppy drives                                |

The two easy mistakes are counting partitions — turning one NVMe drive into
nine "disks" — and filtering on "is it real hardware", which would drop the
`virtio` disk that _is_ a VM's only storage. The filter is therefore about
**whether the entry is a usable block device in its own right**, not about
whether silicon is involved.

A partition is recognised by the `partition` attribute the kernel creates for
it, rather than by pattern-matching names like `nvme0n1p8` — naming schemes
differ between drivers, and a rule built on them would eventually classify a
disk as a partition or the reverse.

### And what counts as a volume

Pseudo-filesystems are filtered: `proc`, `sysfs`, `devtmpfs`, `devpts`,
`cgroup2`, `securityfs`, `tracefs`, `debugfs`, `efivarfs`, `configfs`, `bpf`,
`pstore`, `mqueue`, `hugetlbfs`, `nsfs`, `autofs`, `binfmt_misc`, `fusectl`,
`rpc_pipefs`, `sunrpc`, `squashfs`, `overlay`, `ramfs` and `tmpfs`.

`tmpfs` is the debatable one. `/tmp` and `/dev/shm` are real, useful
filesystems whose usage a user may well want. They are excluded here because
they are **RAM**, already accounted for by the memory metrics, and showing them
beside persistent disks invites the reading that the machine has more storage
than it does. A future mode may surface them explicitly.

### One filesystem mounted several times is one volume

Mounts are grouped by **device number**, which is the kernel's identity for a
mounted filesystem. Fedora's default layout mounts one btrfs filesystem at both
`/` and `/home`; a bind mount adds a third path. All of them are **one volume
with several mount points**, and counting them separately would report the
machine's capacity two or three times over. The mount points are sorted
shortest-first, so the primary one is the path a user thinks of the volume as.

### Attributing a volume to a device

The correlation goes through `/proc/diskstats` on Fedora: the mount's source
names a block device, that device's line gives its `major:minor`, and the disk
whose partitions include that minor is the parent. On Windows it goes through
`IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS`, which asks the volume itself.

**Nothing is inferred from a name.** `nvme0n1p8` _looks_ like a partition of
`nvme0n1`, but a rule built on that would misattribute any driver that names
things differently. A volume PULSE cannot correlate — an NFS export, a volume
spanning two disks — keeps a session-scoped identity, reports no parent, and is
shown in the card's "Other volumes" section. Attributing it to the wrong disk
would be worse than admitting the link is unknown.

---

## Health: NVMe only, for now

### Why only NVMe

NVMe standardises its health log. Log page `0x02` has a fixed 512-byte layout
defined by the specification, and every compliant controller fills the same
fields with the same meaning: composite temperature at offset 1, percentage used
at offset 5, and they mean the same thing on a Samsung, a WD and a Kioxia.

ATA SMART does not work like that. Its attribute table is a list of
vendor-defined IDs whose scaling, normalisation and even direction differ
between manufacturers and firmware revisions — attribute 231 is "SSD life left"
on one drive and "temperature" on another. Normalising it correctly is a
research project of its own, and normalising it _incorrectly_ would publish
confident, wrong numbers about the health of someone's disk.

So a SATA SSD, a hard disk and anything behind a USB bridge reports
`storage.health.*` as `Unsupported`, **with the definitions kept in the
catalog** and a reason that distinguishes "PULSE does not do this yet" from
"your drive does not report it". ATA SMART is deferred, not refused.

### The transport differs; the bytes do not

Linux obtains the 512 bytes through `NVME_IOCTL_ADMIN_CMD`, Windows through
`IOCTL_STORAGE_QUERY_PROPERTY` with `StorageDeviceProtocolSpecificProperty` and
`ProtocolTypeNvme`. Both hand the same buffer to the same parser in
`metrics::wellknown::storage::health`, so the two platforms cannot drift, and
the parser is fully tested on Fedora against synthetic buffers with no NVMe
device in sight.

A buffer shorter than the specified 512 bytes is **refused**, not parsed with
whatever arrived: a short read would publish whatever happened to sit at each
offset as a temperature and a wear figure.

### `percentage_used` is published raw

It is the controller's estimate of how much of the device's rated write
endurance has been consumed. PULSE publishes it exactly as reported:

- The specification **permits values above 100** once the rating has been
  exceeded, and that is precisely the situation a user must be able to see.
  PULSE does not clamp it, and the card marks it rather than hiding it.
- PULSE never computes `100 - percentage_used` and calls it a health score.
  That would report a drive at 155 % used as `−55 %` healthy.

### `available_spare` is not free space

It is the reserve of replacement blocks the controller keeps for retiring worn
ones, as a percentage of what the device shipped with. A healthy drive sits at
or near 100 % for most of its life **while being completely full of data**. It
has nothing to do with free space on a filesystem or with unallocated capacity,
and the metric's description says so.

### `temperature` is the composite figure

The NVMe health log carries a single composite temperature the controller
derives from its own sensors. That is what `storage.health.temperature`
publishes — not one of the controller's individual sensors, and not any other
component's reading.

On Fedora the kernel's `nvme` driver also exposes that same composite figure
through a `hwmon` node that **any user can read**, labelled `Composite`
alongside per-die `Sensor 1`, `Sensor 2` and so on. PULSE finds it by **label**,
never by taking `temp1_input` positionally: on the development machine the
per-die sensors read 52.85 °C and 57.85 °C against a composite of 52.85 °C, and
publishing one of them in its place would quietly report a different sensor
under the same name.

When both sources answer, the controller's own health-log figure wins, so the
two can never disagree.

### Critical Warning

The log's Critical Warning byte is six independent condition bits — spare below
threshold, temperature threshold, degraded reliability, read-only medium,
backup failure, unreliable persistent memory. PULSE **parses and carries** it,
and deliberately publishes **no metric** for it in this phase: the metric
contract has no bitmask or multi-boolean value type yet, and smuggling six facts
into a `Number` or a percentage would be worse than waiting. Folding them into
one score would destroy exactly the information a user needs.

### No verdicts

PULSE will never publish `Storage health: 87/100` or `GOOD 92 %`. It publishes
the measurements the device reports. A future analytics layer may interpret
them; the contract layer does not. A test asserts no storage metric key contains
`score` or `status`, and a card test asserts no verdict wording renders.

---

## Permissions

`/dev/nvme0` is `crw------- root:root` on Fedora. An unprivileged PULSE
therefore gets:

| Value                              | Unprivileged | Why                                      |
| ---------------------------------- | ------------ | ---------------------------------------- |
| Inventory, capacity, bus, identity | yes          | `/sys/class/block` is world-readable     |
| Volumes and their usage            | yes          | `mountinfo` and `statvfs` need no rights |
| I/O counters                       | yes          | `/proc/diskstats` is world-readable      |
| `storage.health.temperature`       | yes          | the kernel's `nvme` `hwmon` node         |
| The other five health values       | **no**       | the admin passthrough needs privilege    |

**PULSE must not require root to start**, so this is not worked around. The
five refused values report `permissionDenied` with an explanation naming
`/dev/nvme0`, and everything else keeps working. Running PULSE as root, or
granting it `CAP_SYS_ADMIN`, makes them appear; no code changes.

Reporting a permission problem as `unsupported` would tell the user their drive
has no health data, which is the opposite of the situation. A test pins that
mapping on both platforms.

---

## Read-only, without exception

| Interface               | What PULSE does with it                                     |
| ----------------------- | ----------------------------------------------------------- |
| `/sys/class/block`      | reads attributes                                            |
| `/proc/diskstats`       | reads                                                       |
| `/proc/self/mountinfo`  | reads                                                       |
| `statvfs(3)`            | queries; the call cannot modify a filesystem                |
| `NVME_IOCTL_ADMIN_CMD`  | **only** `Get Log Page` for log `0x02`                      |
| Windows device handles  | opened with `dwDesiredAccess = 0` — no read or write access |
| Windows device controls | informational queries only                                  |

The NVMe opcode is a **constant**, never a parameter, so no code path
downstream can turn the passthrough into a write command. There is nothing in
PULSE that can issue `Format NVM`, `Sanitize`, `Firmware Commit`, `Namespace
Management` or a destructive self-test, and nothing that mounts, unmounts,
partitions, trims or repairs anything.

### No subprocesses

PULSE runs none of `lsblk`, `blkid`, `df`, `udevadm`, `smartctl`, `nvme`,
`PowerShell`, `wmic`, `diskpart`, `fsutil`, `Get-PhysicalDisk` or `Get-Disk`.
Each of those is a program that reads exactly the interfaces above and formats
the result; spawning one per refresh would add a runtime dependency, a
locale-sensitive output format to parse, and a process, in exchange for
information the kernel already answers directly.

### Sleeping drives

Reading a temperature from a powered-down NVMe controller wakes it. NVMe devices
enter and leave their low-power states in microseconds and PULSE only reads on
an explicit refresh, so this costs nothing measurable. A spun-down **hard disk**
is the case where a health read is genuinely expensive — several seconds and a
mechanical spin-up — and PULSE does not read health from ATA devices at all in
this phase, so the situation does not arise. If an ATA SMART backend is added,
it will have to answer this question explicitly first.

---

## What a refresh costs

Identity, model, serial, WWID, capacity, bus and topology are read **once at
startup** and cached for the life of the process. They cannot change while a
device stays attached, and re-reading seven sysfs files per metric per device on
every refresh — the obvious shape, and the wrong one — would turn a four-disk
machine into a hundred file opens a click.

A refresh on Fedora, for `D` devices, `D_nvme` of them NVMe, and `V` volumes:

```text
1       read     /proc/diskstats            every device's counters, one instant
V       syscall  statvfs                    one per volume actually requested
D_nvme  read     hwmon temp input           composite temperature
D_nvme  ioctl    NVMe health log            when permitted
```

On the development machine — 2 devices, 1 NVMe, 6 volumes — that is **1 file
read, 6 syscalls, 1 hwmon read and 1 refused ioctl**: nine operations for 52
metrics.

Reading every device's counters in one `/proc/diskstats` pass matters for more
than speed: rates are derived from the interval between two snapshots, so every
device's counters must be captured at the _same_ instant, or two disks' figures
describe two slightly different windows.

On Windows there is no single file listing every disk's counters, so it is one
`IOCTL_DISK_PERFORMANCE` per device, issued back to back.

---

## Why not Windows performance counters

`\PhysicalDisk(0 C:)\Disk Reads/sec` is the obvious source and is not used. Its
instance names are **presentation strings** built from a disk number and
whichever drive letters happen to sit on it — they change when a letter is
reassigned, they are localised on some systems, and a disk holding several
volumes produces a name PULSE would have to parse to attribute anything.
Correlating that back to a `StorageDeviceDescriptor` would be a guess dressed as
a measurement.

`IOCTL_DISK_PERFORMANCE` is asked of the device itself, by handle. There is no
name to parse and no correlation to get wrong.

---

## Provider ownership

One provider owns the whole family on each platform:

```text
linux.storage                        windows.storage
 ├── inventory   sysfs                 ├── inventory   SetupAPI
 ├── volumes     mountinfo             ├── volumes     volume GUIDs
 ├── usage       statvfs               ├── usage       GetDiskFreeSpaceExW
 ├── I/O         diskstats             ├── I/O         IOCTL_DISK_PERFORMANCE
 └── health      hwmon + ioctl         └── health      protocol-specific property
```

There is deliberately no `linux.nvme`, `storage.smart` or `filesystem`
provider. The engine rejects two providers claiming the same `MetricRef`, and a
disk's inventory and its health describe the same device — so two providers
would claim the same `SourceId` and the engine would refuse whichever registered
second. Owning the whole family in one provider is what lets the layers be
merged before anything is published, and it is the same reasoning that keeps
NVML inside the GPU provider.

After this phase a supported platform registers **four** providers: CPU, memory,
GPU and storage.

---

## Degradation

Every layer is optional and fails alone:

| What is missing           | What still works                                             |
| ------------------------- | ------------------------------------------------------------ |
| `/proc/diskstats` entry   | inventory, capacity, volumes, health                         |
| NVMe health permission    | inventory, capacity, volumes, I/O, **and the temperature**   |
| Any health backend at all | everything except the six `storage.health.*` values          |
| A device's geometry       | everything except `storage.capacity.total`                   |
| Volume correlation        | the volume, under "Other volumes" rather than the wrong disk |
| All storage               | `storage.device.count` reporting `0`, which is a fact        |

Capabilities are carried **per metric**, never per device. A USB disk whose
bridge hides SMART entirely is still a disk with a capacity, volumes and working
I/O counters — one flag per device is what would make PULSE hide a whole drive
over one missing interface.

---

## Hot-plug

A USB disk can disappear. The provider never panics: a device missing from a
snapshot has its I/O baseline dropped, so one unplugged and reconnected starts
from `NoBaseline` rather than differencing against counters from before it left.
A `statvfs` on a vanished mount point reports `notDetected`, which is honest —
the volume is gone, not broken.

The **inventory** is still built once at startup, so a disk connected after
launch appears on the next launch rather than on the next refresh. Rebuilding it
dynamically is a later phase's work; nothing here breaks in the meantime.

---

## See also

- [`model.md`](model.md) — the metric contract these keys live in
- [`identifiers.md`](identifiers.md) — `MetricKey` and `SourceId` rules
- [`providers.md`](providers.md) — provider ownership and naming
- [`../platforms/fedora.md`](../platforms/fedora.md) — the Fedora interfaces
- [`../platforms/windows.md`](../platforms/windows.md) — the Windows interfaces
