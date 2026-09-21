# Thermal & Cooling Metrics

How PULSE reads temperatures and fan speeds, and — more importantly — which
readings it refuses to publish.

> **PULSE observes. It does not control.**
> Nothing in this phase writes to a `pwm`, a fan policy, a temperature limit or
> a power limit. See [Read-only, without exception](#read-only-without-exception).

---

## The metrics

| Metric                    | Source               | Unit    | Kind  | Value  |
| ------------------------- | -------------------- | ------- | ----- | ------ |
| `cpu.temperature.package` | `cpu:package-N`      | celsius | gauge | number |
| `gpu.temperature.core`    | the GPU's own source | celsius | gauge | number |
| `gpu.temperature.hotspot` | the GPU's own source | celsius | gauge | number |
| `gpu.temperature.memory`  | the GPU's own source | celsius | gauge | number |
| `gpu.fan.speed`           | the GPU's own source | **rpm** | gauge | number |

Where each one comes from:

| Metric                    | Fedora                                                                     | Windows                                                        |
| ------------------------- | -------------------------------------------------------------------------- | -------------------------------------------------------------- |
| `cpu.temperature.package` | `hwmon`: `coretemp` `Package id N`, `k10temp` `Tdie`, `peci_cputemp` `Die` | **unsupported** — no source that works without a kernel driver |
| `gpu.temperature.core`    | NVML, or the card's own `hwmon` node (`amdgpu` `edge`, `nouveau` `temp1`)  | NVML                                                           |
| `gpu.temperature.hotspot` | `amdgpu` `junction`                                                        | unsupported — NVML documents no source                         |
| `gpu.temperature.memory`  | `amdgpu` `mem`                                                             | unsupported — NVML documents no source                         |
| `gpu.fan.speed`           | NVML RPM query, or the card's `fanN_input`                                 | NVML RPM query                                                 |

Every one of these keeps its **definition** on every machine, whatever the
hardware can answer. A metric that cannot be read is published `unsupported`
with a reason, never dropped — so a dashboard built on a machine that has the
sensor still resolves on a machine that does not, and starts working again when
a driver that exposes it is installed.

### Catalog size

```text
9  fixed (memory 4, CPU system 4, gpu.count 1)
3N per logical processor
P  per addressable CPU package
11 per GPU
───
9 + 3N + P + 11G
```

Nothing hardcodes `N`, `P` or `G`, in Rust or in React. On the reference
machine — 32 logical processors, 1 package, 1 GPU — that is **117** metrics.

`P` is _not_ `cpu.count.package`: a machine can know it has two sockets while
PULSE can address neither as a measurement source.

---

## The rules this phase is built on

### Absence is not zero

```text
no sensor        ≠  0 °C
no backend       ≠  a broken application
an approximate   ≠  an exact value
```

A missing reading is an `Availability` with a reason, which the interface shows
as `—` with an explanation. A fabricated `0 °C` would be indistinguishable from
a real one on a chart, and a user cannot tell the difference after the fact.

### A limit is not a temperature

The single most common way a monitoring tool gets this wrong: publishing
`Tjmax`, `Tcontrol`, `Tthrottle` or `temp1_crit` as the current temperature,
and reporting every idle laptop at 100 °C. PULSE reads only `*_input` channels,
and only the ones whose label says they are a current reading.

### A control value is not a measurement

AMD's `Tctl` carries a deliberate offset above the die temperature on many
parts so that stock coolers spin up sooner. It is a control figure, and
publishing it as the CPU's temperature would overstate the machine by a fixed
amount nobody could detect. A part exposing only `Tctl` publishes **nothing**.

### A percentage is not an RPM

Several interfaces report a fan's _duty cycle_ as a percentage. A fan at 40 %
duty may be stopped, spinning up, or sitting on a curve point — the two
quantities are not convertible. `gpu.fan.speed` is declared in RPM and is
published **only** from a value that is genuinely one.

### A core average is not a package temperature

`coretemp` publishes `Core 0`, `Core 4`, `Core 8`… beside `Package id 0`.
Averaging the cores would produce a figure PULSE invented, and one that reads
_lower_ than the truth exactly when it matters: a single core boosting hard is
what throttles a machine, and a mean hides it. PULSE reads the package channel,
or nothing.

### One sensor is not another

`gpu.temperature.core`, `gpu.temperature.hotspot` and `gpu.temperature.memory`
are three physically distinct sensors. The hotspot runs well above the die under
load, and GDDR6X memory runs hotter still. Deriving one from another would
produce numbers that look right and track nothing, so a card that reports one of
the three publishes one of the three.

### 0 RPM is a reading

A GPU below its zero-RPM threshold, or a quiet desktop fan, genuinely turns at
0 RPM, and the user wants to see that. It is shown as `0 RPM`; a fan speed
nothing measured is shown as `—`. Conflating the two is what makes a monitor
untrustworthy in both directions.

---

## Fedora — `hwmon`

Every Linux sensor arrives through one interface, whatever driver is behind it:

```text
/sys/class/hwmon/hwmonN/
├── name                  the driver: coretemp, k10temp, amdgpu, nouveau…
├── device -> ../../…     the hardware this hwmon belongs to
├── temp1_input           a temperature, in millidegrees Celsius
├── temp1_label           what that channel measures, when the driver says
└── fan1_input            a fan speed, in revolutions per minute
```

### `hwmonN` is not an identity

The number is assigned in probe order, changes when a module loads earlier, and
differs between boots. **Nothing is persisted or keyed on it.** A sensor is
identified by the driver `name`, by the hardware its `device` symlink resolves
to, and by the channel's `tempN_label`.

### Millidegrees in, Celsius out

`tempN_input` is in **millidegrees**: `42000` is 42.0 °C. The conversion happens
once, in the platform layer. PULSE's contract carries Celsius, and nothing above
the platform layer — no command, no engine code, no React component — has any
reason to learn that millidegrees exist. `fanN_input` is already RPM.

### Validation

| Reading        | Outcome                                                            |
| -------------- | ------------------------------------------------------------------ |
| `42000`        | 42.0 °C                                                            |
| `0`            | 0.0 °C — a temperature like any other                              |
| `-5000`        | −5.0 °C — cold rooms exist                                         |
| `-400000`      | refused: below absolute zero is a failed read                      |
| `not a number` | refused: a structured parse error                                  |
| file missing   | `temporarilyUnavailable` — a driver reload, a suspend, a GPU reset |

Nothing is silently clamped. The hardware is the source; a reading PULSE merely
finds surprising is published, and only a physically impossible one is refused.

### CPU — which channel is the package

| Driver         | Channel                          | Notes                                                                             |
| -------------- | -------------------------------- | --------------------------------------------------------------------------------- |
| `coretemp`     | `Package id N` → `cpu:package-N` | The label carries the package index, so a dual-socket machine is correct for free |
| `k10temp`      | `Tdie`                           | **Never `Tctl`** (offset control value), never `Tccd*` (chiplet sensors)          |
| `peci_cputemp` | `Die`                            | Never `DTS`, `Tcontrol`, `Tthrottle` or `Tjmax`                                   |

`k10temp` and `peci_cputemp` do not label a package index. PULSE attributes
their reading to package 0 **only** when the machine has exactly one package and
exactly one such device. With two sockets there is no way to tell which device
is which without inventing a mapping out of probe order, so the metric is
`unsupported` with that reason.

Package numbering is the kernel's own `physical_package_id` — the same value
`cpu.count.package` is counted from. PULSE never creates a second,
thermal-only numbering.

### GPU — which channel is which

A card's sensors live under its own device directory, which is what ties them to
_that_ card:

```text
/sys/class/drm/cardN/device/hwmon/hwmonM/
```

| `amdgpu` label | PULSE metric              |
| -------------- | ------------------------- |
| `edge`         | `gpu.temperature.core`    |
| `junction`     | `gpu.temperature.hotspot` |
| `mem`          | `gpu.temperature.memory`  |

**By label, never by position.** Assuming `temp1` is the edge, `temp2` the
junction and `temp3` the memory is right on many cards and wrong on others — and
a card publishing only `edge` and `mem` would have its memory temperature shown
as a hotspot.

A driver that labels nothing is the one exception, and the only case where a
position decides: a device with exactly **one** unlabelled temperature channel
publishes it as the GPU temperature, because a single sensor on a GPU is the
GPU's temperature. Two unlabelled channels publish nothing.

A single `fanN_input` is published as `gpu.fan.speed`. A card exposing several
independent fans publishes none, with the reason — PULSE's contract has one fan
speed per GPU, and reporting fan 1 of three as the adapter's speed would answer
a different question, silently.

---

## NVIDIA — NVML

### Optional symbols

NVML is itself an optional capability; its thermal entry points are an optional
part of it. Both are resolved into an `Option`:

| Symbol                      | Role                                                 |
| --------------------------- | ---------------------------------------------------- |
| `nvmlDeviceGetTemperatureV` | preferred — the current thermal entry point          |
| `nvmlDeviceGetTemperature`  | fallback — deprecated upstream, still widely present |
| `nvmlDeviceGetNumFans`      | how many fans the board reports                      |
| `nvmlDeviceGetFanSpeedRPM`  | the speed, in genuine RPM                            |

The order matters in both directions. `…TemperatureV` is tried first because the
legacy call is deprecated and a future driver may drop it; the legacy call is
used on **any** failure of the first, not only on its absence, so a
versioned-structure mismatch against a driver newer than PULSE's bindings
degrades to a working read rather than an error.

Neither present → `gpu.temperature.core` is `unsupported`, and **every Phase 4
metric keeps working**. An absent temperature costs one metric, never the
NVIDIA backend.

### Hotspot and memory temperature

The public NVML interface PULSE binds documents `NVML_TEMPERATURE_GPU` and
nothing else. `gpu.temperature.hotspot` and `gpu.temperature.memory` are
therefore `unsupported` on NVIDIA, with a reason that says so — they are
separate sensors, and PULSE will not derive one from the die temperature.

### Fan speed

Published only from the RPM query, and only when `nvmlDeviceGetNumFans` reports
exactly one fan:

| Board                     | Outcome                                                                 |
| ------------------------- | ----------------------------------------------------------------------- |
| one fan, RPM readable     | `gpu.fan.speed` available                                               |
| one fan, stopped          | `0 RPM` — a reading                                                     |
| no fan (passively cooled) | `unsupported`, with that reason                                         |
| several independent fans  | `unsupported` — one metric per GPU, and no arbitrary choice             |
| no RPM symbol             | `unsupported` — a duty-cycle percentage is never republished as a speed |

---

## Windows — CPU temperature

**Unsupported, deliberately.**

Windows offers no documented, hardware-independent way to read a processor
package temperature. `MSAcpi_ThermalZoneTemperature` and `Win32_TemperatureProbe`
exist and are not it: an ACPI thermal zone may describe the chassis, the
mainboard or a platform zone, and publishing whichever one answers as _the CPU's_
temperature would be a confident, unfalsifiable lie.

Everything that does read the package — the well-known open-source monitors
among them — ships a kernel-mode driver to reach the model-specific registers.
PULSE does not bundle `WinRing0`, LibreHardwareMonitor, OpenHardwareMonitor or
any kernel helper, and will not add a large vendor SDK for a single reading.

The metric is still **declared**, with `cpu:package-N` sources and an
`unsupported` availability carrying that explanation. A dashboard configured on
Fedora therefore still resolves on Windows and explains itself, and the day
PULSE gains a supported source, the reference it was already publishing starts
carrying values.

AMD and Intel GPU temperatures on Windows are unsupported for the same kind of
reason: no interface PULSE already integrates reports them reliably, and adding
a vendor SDK for one number is not a trade this phase makes.

---

## Read-only, without exception

`hwmon` exposes `pwmN`, `pwmN_enable`, fan curves and temperature limits, and
writing to them changes how the machine cools itself. PULSE:

- never opens a control file, for reading or for writing;
- never changes a PWM mode, a fan policy or a threshold;
- never sets a power limit, an overclock or an undervolt.

This is a monitoring application. Control is not "not implemented yet"; it is
outside what this phase does, and the boundary is deliberate.

---

## Performance

Per refresh, for the thermal metrics:

```text
discovery (once, at startup)
  one walk of /sys/class/hwmon
  one walk of each GPU's <device>/hwmon
  → a path per sensor, cached for the life of the process

sample (per refresh)
  one read of one small file per sensor actually requested
```

A refresh never rescans `/sys/class/hwmon`, never re-resolves an NVML symbol and
never re-enumerates PCI. No file is read twice in one sample: each GPU's
telemetry is gathered once per request however many of its eleven metrics were
asked for, and each package temperature is one `read` of one `tempN_input`.

On the reference machine a full refresh of the thermal family is **one**
file read.

---

## Hotplug and disappearance

A sensor node can vanish: a driver reload, a suspend/resume cycle, a GPU reset,
a device removal. Nothing panics. The read fails, and the failure is classified:

| Cause                         | Availability                            |
| ----------------------------- | --------------------------------------- |
| node gone                     | `temporarilyUnavailable`                |
| unreadable without privileges | `permissionDenied`                      |
| transient I/O error           | `temporarilyUnavailable`                |
| unparseable content           | `providerError` with a structured cause |

A machine whose hardware changes shape entirely — a GPU added, a socket
populated — is rediscovered on the next start. A full hotplug system is not part
of this phase.

---

## Deliberately out of scope

Power draw, voltages, throttling state, per-core CPU temperatures, storage and
mainboard sensors, fan control of any kind, history, and alerting.

`cpu.temperature.core` in particular is **not** published yet, on purpose. The
relationship between a `hwmon` thermal channel, a physical core, a logical
processor and a hybrid P/E topology deserves a mapping done properly. A correct,
portable package temperature comes first.
