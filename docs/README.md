# PULSE Documentation

| Section                                                                            | Contents                                                                                                        |
| ---------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------- |
| [`architecture/overview.md`](architecture/overview.md)                             | Layering, boundaries, modes vs dashboards, the cross-platform rule                                              |
| [`architecture/mini-overlay.md`](architecture/mini-overlay.md)                     | The permanent Mini desktop overlay: requirements and platform reality                                           |
| [`development/getting-started.md`](development/getting-started.md)                 | Prerequisites, setup, commands, workflow                                                                        |
| [`development/testing.md`](development/testing.md)                                 | What is tested automatically, and what must be tested by hand                                                   |
| [`development/msrv.md`](development/msrv.md)                                       | The Rust 1.77.2 minimum, how it is verified, and the pins that keep it                                          |
| [`platforms/fedora.md`](platforms/fedora.md)                                       | Fedora/Linux data sources, permissions, Wayland and X11                                                         |
| [`platforms/windows.md`](platforms/windows.md)                                     | Windows data sources, privileges, packaging caveats                                                             |
| [`metrics/README.md`](metrics/README.md)                                           | The metrics engine: model, providers, catalog, per-family documents                                             |
| [`widgets/README.md`](widgets/README.md)                                           | The original widget contract (see `dashboard/widgets.md` for today's)                                           |
| [`user-guide/README.md`](user-guide/README.md)                                     | End-user documentation: what PULSE does and where to find it                                                    |
| [`processes/inspector.md`](processes/inspector.md)                                 | Process inspector, [provenance](processes/provenance.md) and [controls](processes/controls.md)                  |
| [`history/architecture.md`](history/architecture.md)                               | Persistent history: scheduler, SQLite store, queries, [retention](history/retention.md)                         |
| [`visualization/architecture.md`](visualization/architecture.md)                   | The chart engine: [renderers](visualization/renderers.md), presets, [Customize](visualization/customization.md) |
| [`dashboard/architecture.md`](dashboard/architecture.md)                           | Dashboards: [widgets](dashboard/widgets.md), [grid layout](dashboard/layout.md), import / export                |
| [`overlay/architecture.md`](overlay/architecture.md)                               | Desktop overlays: windows, locking, shortcut, tray, [user guide](overlay/user-guide.md)                         |
| [`design-system/overview.md`](design-system/overview.md)                           | Tokens, hierarchy, density, motion, the eight styles                                                            |
| [`design-system/customization.md`](design-system/customization.md)                 | Appearance studio: every tuning control, saved styles                                                           |
| [`modes/overview.md`](modes/overview.md)                                           | Gaming, Development, Personal and Mini modes                                                                    |
| [`presets/overlay-packs.md`](presets/overlay-packs.md)                             | The twelve built-in overlay packs and footprints                                                                |
| [`presets/dashboard-templates.md`](presets/dashboard-templates.md)                 | The eight built-in dashboard templates                                                                          |
| [`overlay/backends.md`](overlay/backends.md)                                       | Overlay backends and the capability matrix                                                                      |
| [`overlay/gnome-bridge.md`](overlay/gnome-bridge.md)                               | The GNOME Shell bridge: install, status, D-Bus contract, safety                                                 |
| [`overlay/windows-native.md`](overlay/windows-native.md)                           | The Windows native overlay backend (not physically verified)                                                    |
| [`release/ci.md`](release/ci.md)                                                   | CI workflows, jobs, and the three levels of Windows evidence                                                    |
| [`release/windows-ci.md`](release/windows-ci.md)                                   | Native Windows CI, the artifact, checksums, SmartScreen                                                         |
| [`release/windows-physical-validation.md`](release/windows-physical-validation.md) | The Phase 13B physical checklist (not executed)                                                                 |
| [`release/release-process.md`](release/release-process.md)                         | How a release is cut later; why there is no v1 yet                                                              |

## Project status

PULSE is at version `0.1.0-dev` and feature-complete for its first release:
live monitoring on Windows and Fedora Linux, local history, configurable
dashboards, desktop overlays, modes, the appearance studio and process
inspection. Phases 0–13A are done — see [`../CHANGELOG.md`](../CHANGELOG.md).
Native Windows builds are compiled, tested and packaged in CI; the remaining
gate before a public release is the physical Windows checklist
([`release/windows-physical-validation.md`](release/windows-physical-validation.md)).
No release has been published yet.

Many documents above open with the phase that introduced their subject. That
heading records history; the content describes the current implementation.
