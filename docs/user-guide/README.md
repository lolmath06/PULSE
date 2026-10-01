# PULSE User Guide

## What PULSE is

A cross-platform system monitor for **Windows** and **Fedora Linux**. It shows
what the machine is doing — processor, graphics, memory, storage, network and
processes — and lets you decide how: on the Overview, on dashboards you
compose, in the small Mini window, or as overlays on the desktop.

## Where things are

| Section         | What you find there                                                                                       |
| --------------- | --------------------------------------------------------------------------------------------------------- |
| **Overview**    | A live strip, the four modes, and every system section with its details and history — including processes |
| **Dashboard**   | Your dashboards: add, move, resize and customise widgets; start from one of eight templates               |
| **Overlays**    | Widgets in their own windows on the desktop; twelve ready-made packs; the overlay backend's status        |
| **Gaming**      | Overlay-first monitoring next to a game — never inside it                                                 |
| **Development** | Build pressure: CPU, memory, disk I/O, network and process counts in a dense layout                       |
| **Personal**    | A balanced, freely composable everyday view                                                               |
| **Mini**        | A small, ordinary PULSE window with its own layouts or any dashboard                                      |
| **Appearance**  | Eight styles, deep tuning, and saved styles of your own                                                   |

The first launch opens a short, skippable welcome: a mode, a style, a starter
dashboard and an overlay. Everything chosen there can be changed later.

## Guides

- [Modes](../modes/overview.md) — Gaming, Development, Personal and Mini
- [Overlays](../overlay/user-guide.md) — creating, editing, locking, the shortcut and the tray
- [Dashboard templates](../presets/dashboard-templates.md) and [overlay packs](../presets/overlay-packs.md)
- [Appearance studio](../design-system/customization.md)
- [Process inspector](../processes/inspector.md) and [process controls](../processes/controls.md)
- [History and retention](../history/retention.md)

## When a value is missing

Not every machine can report every metric. PULSE shows why — not supported on
this platform, not present on this machine, needs more permissions, or
temporarily unavailable — instead of showing a made-up value.

## Installing

No release has been published yet. To run PULSE from source, see
[`../development/getting-started.md`](../development/getting-started.md).
Unsigned Windows test builds are produced by CI — see
[`../release/windows-ci.md`](../release/windows-ci.md).
