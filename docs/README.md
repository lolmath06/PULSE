# PULSE Documentation

| Section                                                                            | Contents                                                               |
| ---------------------------------------------------------------------------------- | ---------------------------------------------------------------------- |
| [`architecture/overview.md`](architecture/overview.md)                             | Layering, boundaries, modes vs dashboards, the cross-platform rule     |
| [`architecture/mini-overlay.md`](architecture/mini-overlay.md)                     | The permanent Mini desktop overlay: requirements and platform reality  |
| [`development/getting-started.md`](development/getting-started.md)                 | Prerequisites, setup, commands, workflow                               |
| [`development/testing.md`](development/testing.md)                                 | What is tested automatically, and what must be tested by hand          |
| [`development/msrv.md`](development/msrv.md)                                       | The Rust 1.77.2 minimum, how it is verified, and the pins that keep it |
| [`platforms/fedora.md`](platforms/fedora.md)                                       | Fedora/Linux data sources, permissions, Wayland and X11                |
| [`platforms/windows.md`](platforms/windows.md)                                     | Windows data sources, privileges, packaging caveats                    |
| [`metrics/README.md`](metrics/README.md)                                           | The planned metrics engine contract                                    |
| [`widgets/README.md`](widgets/README.md)                                           | The planned widget contract                                            |
| [`user-guide/README.md`](user-guide/README.md)                                     | End-user documentation (grows with the features)                       |
| [`design-system/overview.md`](design-system/overview.md)                           | Tokens, hierarchy, density, motion, the eight styles                   |
| [`design-system/customization.md`](design-system/customization.md)                 | Appearance studio: every tuning control, saved styles                  |
| [`modes/overview.md`](modes/overview.md)                                           | Gaming, Development, Personal and Mini modes                           |
| [`presets/overlay-packs.md`](presets/overlay-packs.md)                             | The twelve built-in overlay packs and footprints                       |
| [`presets/dashboard-templates.md`](presets/dashboard-templates.md)                 | The eight built-in dashboard templates                                 |
| [`overlay/backends.md`](overlay/backends.md)                                       | Overlay backends and the capability matrix                             |
| [`overlay/gnome-bridge.md`](overlay/gnome-bridge.md)                               | The GNOME Shell bridge: install, status, D-Bus contract, safety        |
| [`overlay/windows-native.md`](overlay/windows-native.md)                           | The Windows native overlay backend (not physically verified)           |
| [`release/ci.md`](release/ci.md)                                                   | CI workflows, jobs, and the three levels of Windows evidence           |
| [`release/windows-ci.md`](release/windows-ci.md)                                   | Native Windows CI, the artifact, checksums, SmartScreen                |
| [`release/windows-physical-validation.md`](release/windows-physical-validation.md) | The Phase 13B physical checklist (not executed)                        |
| [`release/release-process.md`](release/release-process.md)                         | How a release is cut later; why there is no v1 yet                     |

## Project phases

- **Phase 0 — Foundation** _(current)_: project structure, platform
  abstraction, one real React ↔ Rust command, navigation shell, docs, CI.
- **Phase 1+**: metrics engine, real monitoring, graphs, history, widget engine,
  dashboards, Mini overlay, themes, tray, packaging.

Everything beyond Phase 0 is deliberately out of scope for now; the documents
above describe the _contracts_ those phases must satisfy, not their
implementation.
