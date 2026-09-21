# Network Metrics

How PULSE inventories network interfaces, measures their traffic, and reports
what a Wi-Fi radio says about its link — and, as always, which numbers it
refuses to publish.

> **PULSE observes. It does not control, and it does not probe.**
> Nothing here brings a link up or down, configures an address, joins a
> network or changes any setting. Nothing here sends a packet: no ping, no DNS
> query, no speed test, no public-IP lookup. Every figure is a counter the
> operating system was already keeping.

---

## Receive and transmit, from the machine's point of view

`receive` is what arrives at this machine; `transmit` is what leaves it. The
interface renders them as **Download** and **Upload**.

This is easy to get backwards and produces a monitor that is confidently
wrong, so the direction is stated in every metric description, in the card's
own footnote, and in a contract test that asserts both descriptions say it.

---

## The metrics

For `I` published interfaces of which `W` are Wi-Fi, the catalog holds
`2 + 11I + 4W` network metrics. `I` **excludes loopback** — see below. Nothing
hardcodes `I` or `W`.

### Machine-wide — `network:system`

| Metric                       | Unit  | Kind  | Value  |
| ---------------------------- | ----- | ----- | ------ |
| `network.interface.count`    | count | state | number |
| `network.interface.up_count` | count | gauge | number |

The count is a `state` because it is a discrete fact about the machine; the
up-count is a `gauge` because it genuinely rises and falls as cables and
networks come and go, so averaging it over time is meaningful.

### Per interface — the interface's own source

| Metric                                | Unit             | Kind  | Value  |
| ------------------------------------- | ---------------- | ----- | ------ |
| `network.receive.bytes_per_second`    | bytesPerSecond   | gauge | number |
| `network.transmit.bytes_per_second`   | bytesPerSecond   | gauge | number |
| `network.receive.packets_per_second`  | packetsPerSecond | gauge | number |
| `network.transmit.packets_per_second` | packetsPerSecond | gauge | number |
| `network.receive.errors_per_second`   | packetsPerSecond | gauge | number |
| `network.transmit.errors_per_second`  | packetsPerSecond | gauge | number |
| `network.receive.dropped_per_second`  | packetsPerSecond | gauge | number |
| `network.transmit.dropped_per_second` | packetsPerSecond | gauge | number |
| `network.link.receive_speed`          | bitsPerSecond    | gauge | number |
| `network.link.transmit_speed`         | bitsPerSecond    | gauge | number |
| `network.mtu`                         | bytes            | state | number |

### Per Wi-Fi interface — **only** on a wireless interface

| Metric                            | Unit              | Kind  | Value  |
| --------------------------------- | ----------------- | ----- | ------ |
| `network.wifi.signal.quality`     | percent           | gauge | number |
| `network.wifi.signal.rssi`        | decibelMilliwatts | gauge | number |
| `network.wifi.link.receive_rate`  | bitsPerSecond     | gauge | number |
| `network.wifi.link.transmit_rate` | bitsPerSecond     | gauge | number |

### Where each one comes from

| Layer            | Fedora                                | Windows                                     |
| ---------------- | ------------------------------------- | ------------------------------------------- |
| Inventory        | `rtnetlink` `RTM_GETLINK`             | `GetIfTable2` / `MIB_IF_ROW2`               |
| Identity         | `IFLA_PERM_ADDRESS`, `IFLA_ADDRESS`   | `PermanentPhysicalAddress`, `InterfaceGuid` |
| Traffic counters | `IFLA_STATS64`, in the same dump      | the same table                              |
| Link state       | `IFLA_OPERSTATE` + `IFF_UP`           | `OperStatus` + `MediaConnectState`          |
| MTU              | `IFLA_MTU`                            | `Mtu`                                       |
| Link speed       | `/sys/class/net/<iface>/speed`        | `ReceiveLinkSpeed`, `TransmitLinkSpeed`     |
| Addresses        | `rtnetlink` `RTM_GETADDR`             | not read in this phase                      |
| Which are Wi-Fi  | `nl80211` `NL80211_CMD_GET_INTERFACE` | `NDIS_PHYSICAL_MEDIUM`                      |
| Wi-Fi link       | `nl80211` `NL80211_CMD_GET_STATION`   | WLAN realtime connection quality            |

---

## Three new units

`network.*` needed three additions to the shared contract, and none is
cosmetic:

| Unit                | Why it is not an existing one                                                                                                                                                                                                                                                           |
| ------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `packetsPerSecond`  | Deliberately **not** `operationsPerSecond`. A disk operation and a network packet come from different subsystems and differ by orders of magnitude on the same machine; letting them share an axis invites comparing 900 IOPS with 900 packets/s as if the numbers meant the same thing |
| `bitsPerSecond`     | The unit of **link capacity**. A 1 Gbit/s link carrying 12 MiB/s is one number in bits and one in bytes, and conflating them is a factor-of-eight error that looks entirely plausible                                                                                                   |
| `decibelMilliwatts` | A **logarithmic** scale, always negative in practice. Publishing it as `none` would let a widget average two readings arithmetically, or scale it from zero                                                                                                                             |

A contract test asserts no network metric borrows `operationsPerSecond`, and
that traffic is in bytes while every link figure is in bits.

### Link capacity is bits; observed traffic is bytes

This is the one place PULSE deliberately uses two conventions side by side,
and the card renders them differently on purpose:

```text
Download    12.4 MiB/s      binary prefixes, bytes  — what is flowing
Link RX      1.0 Gbit/s     decimal prefixes, bits  — what the link can carry
```

Networking counts capacity in powers of ten and always has: a "gigabit" link
is 1 000 000 000 bits per second, and every switch, driver and datasheet
agrees. Rendering it with binary prefixes would show an ordinary gigabit link
as `0.93 Gibit/s`, which matches nothing the user has ever seen.

---

## Traffic is measured, not read

Throughput, packet rates, error rates and drop rates are **rates**. Both
operating systems expose monotonic totals — octets and packets since the
interface came up — so a single absolute read says nothing:

```text
delta bytes   / elapsed seconds  →  bytes per second
delta packets / elapsed seconds  →  packets per second
```

The arithmetic lives once, in `metrics::wellknown::network::traffic`, and is
shared by both platforms. The platform backends supply eight totals per
interface and nothing else.

### Why not the storage tracker

The shape is the same; the data is not. A disk has six counters including two
service-time accumulators that produce a latency, and an interface has eight
including errors and drops that produce nothing of the kind. Forcing network
counters through `StorageIoTracker` would mean carrying two dead fields and a
latency concept with no meaning here, to save about forty lines of arithmetic
that is trivial to test directly.

What _is_ shared, deliberately, is the state machine's **semantics**, so a
user sees the same distinction between "waiting for a second sample",
"measured zero" and "cannot be measured" on a disk and on a network adapter.

### Zero is a measurement, absence is not

| Situation                     | What PULSE publishes                                             |
| ----------------------------- | ---------------------------------------------------------------- |
| Idle interface, real interval | `0 B/s`, `0 pkt/s`, `0 errors/s`, `0 drops/s` — **measurements** |
| First sample, no baseline     | **no value**, with "waiting for another sample"                  |
| Counters went backwards       | **no value**, baseline restarted                                 |

Publishing `0 B/s` before a baseline exists would report an unmeasured
interface as idle, which the user cannot distinguish from a genuinely idle
one. The card says _Waiting for another sample_ and does **not** fire a hidden
second request to paper over it — that would be a polling loop with one
iteration, and it would hide the thing that makes the numbers trustworthy.

### Counters that go backwards

An interface taken down and brought back up, a driver reset, a recreated
virtual interface, a suspend/resume cycle, or Windows's 32-bit packet counters
wrapping all make a total go _down_. Any single field regressing invalidates
the whole snapshot as a baseline: a reset does not clear counters selectively,
and differencing the fields that happen to still be ascending would publish a
throughput derived from two different eras of the link's life.

---

## Errors and drops are different counters

| Counter    | What it means                                                                                        |
| ---------- | ---------------------------------------------------------------------------------------------------- |
| **Errors** | A frame that arrived **broken** — a bad checksum, a framing error, a length violation                |
| **Drops**  | A frame that arrived **intact and was discarded** — a full buffer, or no protocol handler wanting it |

A persistent non-zero error rate usually means a cable or a driver problem. A
non-zero drop rate frequently means nothing at all: a machine receiving
broadcast traffic for protocols it does not speak drops frames continuously
and correctly.

### And neither is Internet packet loss

`rx_dropped` counts frames this machine discarded _after receiving them_. It
says nothing about packets lost between here and a server on the other side of
the world. Reporting it as "packet loss" would be a category error, and the
metric's own description says so.

**Measuring real packet loss needs an active probe** — sending packets to a
chosen target and counting what comes back — which is a different kind of
feature with a target, a cadence and a privacy question attached. Ping, jitter,
latency to an external host, DNS timing and speed tests are all deliberately
out of scope for this phase. See [Deliberately absent](#deliberately-absent).

---

## Link speed: zero is not a speed

Both platforms use a special value for "the driver has no answer", and it is
not the same value:

| Platform | "Unknown" is                                 | Where                                    |
| -------- | -------------------------------------------- | ---------------------------------------- |
| Fedora   | `-1` in the file; `EINVAL` on read           | `/sys/class/net/<iface>/speed`           |
| Windows  | `0`, and `u64::MAX` on some virtual adapters | `ReceiveLinkSpeed` / `TransmitLinkSpeed` |

None of them is published. A link speed of zero would claim a connection with
no capacity at all, and `u64::MAX` is not an exabit-per-second link. PULSE
publishes nothing and the card shows `—` with the reason.

A `veth` pair, meanwhile, genuinely reports `10000` — 10 Gbit/s, being a
software device with no physical limit. That is what the kernel says, so it is
what PULSE reports.

### Why sysfs rather than ethtool netlink on Fedora

The structured answer is `ETHTOOL_MSG_LINKMODES_GET` over its own generic
netlink family. It carries far more than PULSE needs — every supported and
advertised link mode, autonegotiation state, lane counts — and using it would
mean resolving a third netlink family and writing a third attribute mapping,
for **one number** that the kernel already exposes as a one-line file.

The cost is one `open`/`read`/`close` per _Ethernet_ interface per refresh: on
the development machine, one. That is a different trade from the disk counters
in Phase 6, where a file per counter per device meant a hundred opens at a
hundred different instants — a link speed is not a rate and does not need to be
captured at the same instant as anything else.

---

## Signal quality is not RSSI

An RSSI of −68 dBm is a **measurement**: the power arriving at the antenna, on
a logarithmic scale, as the radio reports it. A "signal quality" of 92 % is an
**interpretation**: somebody decided which dBm value counts as 0 and which
counts as 100, and different vendors decide differently.

Every popular formula is arbitrary. The common `quality = 2 × (dBm + 100)`
maps −100 dBm to 0 % and −50 dBm to 100 %, making a perfectly ordinary
−55 dBm link read as 90 % and a marginal −85 dBm link read as 30 %; Windows's
own mapping is different again. A number produced that way would look like a
measurement and be a guess.

So PULSE publishes a quality percentage **only when the platform itself
computes one**:

| Platform | RSSI              | Quality                                                                                           |
| -------- | ----------------- | ------------------------------------------------------------------------------------------------- |
| Windows  | from the WLAN API | **yes** — `ulLinkQuality` is documented as 0–100 and the OS owns the mapping                      |
| Fedora   | from `nl80211`    | **no** — `cfg80211` reports dBm and nothing else, so the metric is `unsupported` with that reason |

A user on Fedora sees a real RSSI and an honest "this platform does not report
a quality percentage". That is more useful than a fabricated 92 %.

### Multi-link operation

Wi-Fi 7 radios can associate over several links at once, and both `nl80211`
and the Windows realtime-quality API can report per-link figures.

**dBm cannot be averaged arithmetically** — it is logarithmic, so the mean of
−50 and −90 is not −70 in any meaningful sense — and taking `links[0]` would
silently report whichever link the driver happened to list first.

PULSE publishes the **strongest active link's** RSSI, and its negotiated rates,
and says so in the metric's description. That is a defined, reproducible rule:
it answers "how good is this machine's radio link right now" with the best
evidence available, and degrades to exactly the single-link answer on every
radio that has one link.

---

## Identity

Every obvious candidate is wrong, as it was for GPUs and disks:

| Candidate                  | Why it must not be an identity                                         |
| -------------------------- | ---------------------------------------------------------------------- |
| `eth0`, `wlan0`            | Kernel enumeration order. A second adapter renames the first           |
| `Ethernet`, `Wi-Fi`        | Windows _display_ names, and the user can rename them                  |
| `InterfaceIndex`/`ifindex` | Documented as unstable, and **reused** after an interface is destroyed |
| An IP address              | Assigned by DHCP, changes per network, and several may exist at once   |
| The **current** MAC        | Randomised on Wi-Fi by default on both platforms — see below           |

### The MAC randomisation trap

A Wi-Fi interface's current hardware address is frequently not the one burned
into the adapter. **Both NetworkManager and Windows randomise it per network by
default**, as an anti-tracking measure, and it changes when the machine joins a
different SSID. An identity built on it would give a user's Wi-Fi widget a new
identity every time they moved between home and the office.

Both platforms expose the real one separately — `IFLA_PERM_ADDRESS` on Linux,
`PermanentPhysicalAddress` on Windows — and that is what PULSE prefers.

### What PULSE uses instead

| Mechanism          | Source                                           | Stability       |
| ------------------ | ------------------------------------------------ | --------------- |
| `permanent-mac`    | `IFLA_PERM_ADDRESS` / `PermanentPhysicalAddress` | **Hardware**    |
| `system-id`        | a Windows interface GUID                         | SystemAssigned  |
| `current-mac`      | `IFLA_ADDRESS` / `PhysicalAddress`               | CurrentAddress  |
| `predictable-name` | `enp58s0`, `wlp59s0f0`                           | PredictableName |
| `interface-name`   | any other name                                   | **Session**     |

The stability is **recorded, not assumed**: `IdentityStability` is carried on
every descriptor, and distinguishes what survives a reboot from what survives
moving to another operating system.

### One identifier across operating systems

A permanent MAC is six bytes burned into the adapter, so Fedora and Windows
read the _same value_ off the same card. Normalised the same way, they produce
the same `SourceId`:

```text
network:mac-9009df3e97f2
```

A dashboard widget bound to a laptop's Wi-Fi radio therefore survives a dual
boot. A contract test asserts it, driving both platforms' identity code with
the same synthetic adapter — including one whose current address differs from
its permanent one on each side.

Windows's interface GUID ranks _below_ the permanent MAC precisely because it
is a Windows construct with no Linux counterpart.

### Addresses that mean "there is none"

`IFLA_PERM_ADDRESS` and `PermanentPhysicalAddress` report **all zeros** for an
interface with no permanent address, such as a tunnel. Accepting it would
collapse every WireGuard, TUN and VPN interface on the machine onto
`network:mac-000000000000`. All-zero and all-ones addresses are refused, as are
multicast ones — an interface cannot be assigned a multicast address, so the
bit being set means the bytes were misread.

A locally-administered address is **accepted**: a Docker bridge's `02:42:…`
genuinely belongs to that bridge and is stable for as long as it exists.

---

## What counts as an interface

### Loopback is excluded

`lo` and `Loopback Pseudo-Interface 1` are discovered internally and **never
published**. They always exist and only ever carry traffic that never left the
machine; including them would make "how much is this machine downloading"
answerable only after mentally subtracting a number.

The filter lives in the **shared** declaration code, so neither platform has to
remember it and the two cannot disagree. A contract test asserts that a
loopback descriptor produces exactly the two machine-wide metrics and nothing
else on both platforms.

### Nothing else is hidden

VPNs, WireGuard, Tailscale, TUN/TAP, bridges, Docker networks, `veth` pairs,
Hyper-V switches — all inventoried, all published, all measured. Several of
them are exactly what a user wants to see.

What the _interface_ does is **group** them, because a machine running
containers can have thirty `veth` pairs and two real adapters:

```text
Ethernet, Wi-Fi, Tunnel   shown as cards
Bridge, Virtual, Other    collapsed behind "Show all (N)"
```

Collapsed, never hidden, and never removed from the catalog. A saved dashboard
widget bound to a bridge keeps working whether the section is expanded or not.

### Kind is never guessed from the name

`wlan0` is a convention that nothing enforces, a bridge can be called `eth0`,
and — the decisive point — **every Wi-Fi station reports `ARPHRD_ETHER` on
Linux and frequently `IF_TYPE_ETHERNET` on Windows**, exactly like a wired NIC.

So PULSE asks:

- **Fedora** — `nl80211` for its own list of wireless interface indices, and
  `IFLA_LINKINFO`'s `IFLA_INFO_KIND` for `bridge`, `veth`, `wireguard`, `tun`
  and the rest;
- **Windows** — `NDIS_PHYSICAL_MEDIUM` for wireless, and the
  `HardwareInterface` flag to tell a real Ethernet port from a Hyper-V switch
  that reports Ethernet without being hardware.

A test drives the same Wi-Fi fixture through both paths and asserts it is
classified as Ethernet when `nl80211` does not claim it and as Wi-Fi when it
does — from the same bytes.

---

## Wi-Fi metrics exist only on Wi-Fi interfaces

Unlike the storage health metrics, which are declared on every device and
marked `unsupported` where they cannot be read, the four `network.wifi.*` keys
are declared **only for wireless interfaces**.

The distinction is between _a capability a device might have and does not_ and
_a concept that does not apply at all_. An NVMe health log is something a SATA
drive could conceivably report, so keeping the definition tells the user
something. An RSSI on an Ethernet port is not a missing measurement — there is
no radio — and on the development machine, declaring them everywhere would add
**forty permanently-unsupported definitions** for seven bridges and three
`veth` pairs.

A Wi-Fi interface that is _present but not associated_ keeps all four
definitions, carrying `temporarilyUnavailable`: the radio is there, there is
simply nothing to measure until it joins a network, and that resolves itself.

---

## Privacy

### No SSID, no BSSID, anywhere

PULSE reads **link quality**, not **network identity**. Nothing in this phase
requests, parses, stores or displays an SSID or a BSSID.

On Windows this is a concrete design constraint rather than a preference. The
obvious source for a signal strength is
`wlan_intf_opcode_current_connection`, which returns signal quality, rates,
**and** the SSID and BSSID. On recent Windows those last two make the call
subject to the machine's **location permission**, because a BSSID is a
geolocation primitive — given one, a lookup service places the device within a
few metres.

So PULSE asks `wlan_intf_opcode_realtime_connection_quality`, which reports
link quality, per-link RSSI and negotiated rates and carries no network
identity at all.

### And no fallback to the location-gated call

That opcode is recent. On a Windows build that does not implement it, the query
fails and PULSE reports the four Wi-Fi metrics as unavailable with that
reason — it does **not** quietly fall back to the connection API. Falling back
would work, and would mean a monitoring tool silently reaching for a
location-gated interface behind the user's back to obtain a number it had just
said it could not get. The generic interface metrics are unaffected either way.

Two tests guard this: one greps this module for the refused API names (with the
needles assembled at runtime so the test's own source does not trip it), and
one asserts the parsed types have no field an SSID could be stored in.

### What is shown

| Shown                                | Not shown                                                                           |
| ------------------------------------ | ----------------------------------------------------------------------------------- |
| The interface name and kind          | The full MAC address in the card's text                                             |
| Local IPv4 and global IPv6 addresses | Link-local `fe80::` addresses, which say nothing about which network the user is on |
| Signal, quality, rates, traffic      | SSID, BSSID, any public IP, any location                                            |

The stable identity — which _is_ derived from a MAC — remains available in the
row's tooltip for debugging. IP addresses are display-only and never reach a
`SourceId`.

---

## Read-only, without exception

| Interface                      | What PULSE does with it                                                  |
| ------------------------------ | ------------------------------------------------------------------------ |
| `AF_NETLINK` socket            | binds with **no multicast group**; receives only replies to what it sent |
| `RTM_GETLINK`, `RTM_GETADDR`   | dumps — reads                                                            |
| `nl80211`                      | `GET_INTERFACE` and `GET_STATION` — reads                                |
| `/sys/class/net/<iface>/speed` | reads                                                                    |
| `GetIfTable2`                  | reads; freed with `FreeMibTable`                                         |
| WLAN API                       | `WlanQueryInterface` only; buffers freed with `WlanFreeMemory`           |

There is no code path that configures an address, changes an MTU, brings a link
up or down, joins or leaves a network, or sends a single packet.

### No subprocesses

PULSE runs none of `ip`, `ifconfig`, `ethtool`, `iw`, `iwconfig`, `nmcli`,
`networkctl`, `netstat`, `ss`, `PowerShell`, `netsh`, `wmic` or `ipconfig`.
Each is a program that opens the same interfaces above and formats the result;
spawning one per refresh would add a runtime dependency on packages Fedora does
not always install, a locale-sensitive output format to parse, and a process.

---

## Parsing netlink safely

Two data sources speak netlink, and PULSE parses the framing itself. The
alternative crates are either large (`neli`), asynchronous and therefore a
Tokio dependency PULSE does not otherwise have (`rtnetlink`), or a stack of
four or five from the `rust-netlink` family — against **one attribute iterator
and two header structs**, shared between both families.

The decisive argument is the one that applied to the NVMe log and the Windows
device descriptors in Phase 6: a pure `&[u8] -> value` parser is testable on
any machine against any byte sequence, including malformed ones. A crate would
move that surface out of PULSE's test suite without removing it.

Every bound is checked, and the fuzz-shaped cases are unit tests rather than
hopes:

| Malformed shape                          | What PULSE does                                                             |
| ---------------------------------------- | --------------------------------------------------------------------------- |
| Attribute length below its 4-byte header | Terminates the walk. **Would otherwise hang** — the cursor advances by zero |
| Attribute claiming more than the buffer  | Refused; the walk stops                                                     |
| Truncated final attribute                | Dropped; the intact ones survive                                            |
| Message length below the 16-byte header  | Refused, same hang reason                                                   |
| Multipart reply that never terminates    | Bounded at 64 datagrams, then a `Timeout` error                             |
| A reply to a _previous_ request          | Ignored — replies are matched on sequence number                            |
| Payload narrower than the type read      | Every accessor returns `None` rather than reading past it                   |
| Invalid UTF-8 in a name                  | Refused rather than mangled                                                 |

A malformed message becomes a `MetricError`, never a crash and never undefined
behaviour. The nesting and byte-order flags are masked off before a type is
compared, because kernels set them inconsistently and a parser comparing raw
values silently fails to find half the nested attributes.

### Units that are easy to get wrong

| Field                         | Unit                   | The misreading it invites                             |
| ----------------------------- | ---------------------- | ----------------------------------------------------- |
| `NL80211_RATE_INFO_BITRATE32` | 100 kbit/s             | `1755` is 175.5 Mbit/s, not 1755 bit/s or 1755 Mbit/s |
| `NL80211_STA_INFO_SIGNAL`     | **signed** byte, dBm   | `0xBC` is −68, not 188                                |
| WLAN `ulRxRate` / `ulTxRate`  | kilobits/s             | `866700` is 866.7 Mbit/s                              |
| WLAN `lRssi`                  | **signed** 32-bit, dBm | −54 stored as `LONG` is `0xFFFFFFCA`                  |
| `/sys/.../speed`              | megabits/s             | `1000` is 1 Gbit/s                                    |

Each conversion happens once, in one place, and is tested.

---

## Windows packets are two counters

Windows splits packet counts into unicast and non-unicast:

```text
RX packets = InUcastPkts  + InNUcastPkts
TX packets = OutUcastPkts + OutNUcastPkts
```

Publishing only the unicast half — the obvious reading, since it is the field
whose name looks like "packets" — silently drops every broadcast and multicast
frame, which on a normal network is a large and variable share of the total.
The addition is checked, because two near-maximum counters must not wrap into a
small number.

A test feeds the same activity through both platforms' counter mapping and
asserts they produce identical `NetworkCounters`.

---

## Why not PDH on Windows

`\Network Interface(*)\Bytes Received/sec` is the obvious source and is not
used, for the same reason `\PhysicalDisk` was not used for storage: its
instance names are presentation strings derived from the adapter description,
they are localised, they are mangled — parentheses and slashes are rewritten —
and correlating one back to a `MIB_IF_ROW2` means matching munged text.

`GetIfTable2` is asked of the stack directly and returns the identity alongside
the counters, so there is nothing to correlate.

---

## What a refresh costs

Identity, name, kind and permanent address are read **once** when the provider
is built. They cannot change while an interface exists.

**Fedora**, for `I` interfaces of which `E` are Ethernet and `W` are Wi-Fi:

```text
1   netlink transaction   RTM_GETLINK    every interface, every counter, one instant
1   netlink transaction   RTM_GETADDR    every local address
E   file reads            speed          one per Ethernet interface
W   netlink transactions  nl80211 station dump
```

On the development machine — thirteen interfaces, one Wi-Fi, one Ethernet —
that is **three netlink round trips and one file read** for thirteen
interfaces' worth of metrics. The alternative shape, one
`/sys/class/net/<iface>/statistics/<counter>` read per metric, would be **104
file opens at 104 slightly different instants**.

**Windows**:

```text
1   GetIfTable2       every interface, every counter, one instant
1   WLAN query set    only when the request touches a wireless interface
```

Reading every interface's counters in one transaction matters for more than
speed: rates are derived from the interval between two snapshots, so every
interface's counters must be captured at the **same** instant, or two adapters'
figures describe two slightly different windows.

---

## Degradation

Every layer is optional and fails alone:

| What is missing                  | What still works                                          |
| -------------------------------- | --------------------------------------------------------- |
| `nl80211` (no wireless hardware) | Every generic metric; no interface is classified as Wi-Fi |
| The address dump                 | Every counter, state and link figure                      |
| A Wi-Fi station query            | That interface's traffic, state, MTU                      |
| The Windows WLAN service         | The entire generic catalog                                |
| The realtime-quality opcode      | The entire generic catalog                                |
| One interface that vanished      | Every other interface                                     |
| All networking                   | `network.interface.count` reporting `0`, which is a fact  |

Capabilities are carried **per metric**, never per interface: a tunnel has
perfectly good byte counters and no link speed, and a bridge reports no errors
because it has no physical layer to have them on.

---

## Hot-plug

The inventory is built **once**, when the provider is created. A USB adapter
plugged in afterwards, or a VPN that connects, appears on the **next launch**
rather than the next refresh.

Nothing breaks in the meantime: an interface that disappears has its baseline
dropped, so one unplugged and reconnected starts from `NoBaseline` rather than
differencing against counters from before it left, and a counter lookup that
finds nothing degrades to `temporarilyUnavailable` rather than a panic.
Interfaces are matched between refreshes on **index _and_ name** on Linux, and
on **LUID** on Windows, so a reused index cannot publish a newcomer's counters
under the old interface's identity.

Rebuilding the inventory dynamically is a later phase's work.

---

## Deliberately absent

This phase is **passive**. It reads counters the operating system was already
keeping and sends nothing.

| Not measured                       | Why                                                                 |
| ---------------------------------- | ------------------------------------------------------------------- |
| Ping / round-trip latency          | Needs an active probe against a chosen target                       |
| Jitter                             | Likewise, and a sampling cadence                                    |
| **Internet packet loss**           | Likewise. `rx_dropped` is a _local_ counter and is not this         |
| DNS resolution time                | Needs a query against a chosen resolver                             |
| Throughput capacity ("speed test") | Needs to generate traffic, which a monitor should not do            |
| Public IP, geolocation             | Needs a third-party service, and is network _identity_, not quality |
| Per-process network usage          | A different subsystem, and expensive                                |
| History and graphs                 | No scheduler or ring buffer yet, on any metric family               |

Each of those needs a target, a cadence and a privacy decision. They are a
future **active** capability, and keeping them out of the passive foundation is
what lets this phase ship without one.

---

## No verdicts

PULSE will never publish `Network health: 93 %` or a connection score. It
publishes the measurements the operating system reports. A future analytics
layer may interpret them; the contract layer does not. Tests assert no network
metric key contains `score` or `health`, and that no verdict wording renders in
the card.

---

## See also

- [`model.md`](model.md) — the metric contract these keys live in
- [`identifiers.md`](identifiers.md) — `MetricKey` and `SourceId` rules
- [`providers.md`](providers.md) — provider ownership and naming
- [`storage.md`](storage.md) — the same baseline/delta design, one phase earlier
- [`../platforms/fedora.md`](../platforms/fedora.md) — the Fedora interfaces
- [`../platforms/windows.md`](../platforms/windows.md) — the Windows interfaces
