# PULSE Documentation

| Section                                                            | Contents                                                              |
| ------------------------------------------------------------------ | --------------------------------------------------------------------- |
| [`architecture/overview.md`](architecture/overview.md)             | Layering, boundaries, modes vs dashboards, the cross-platform rule    |
| [`architecture/mini-overlay.md`](architecture/mini-overlay.md)     | The permanent Mini desktop overlay: requirements and platform reality |
| [`development/getting-started.md`](development/getting-started.md) | Prerequisites, setup, commands, workflow                              |
| [`development/testing.md`](development/testing.md)                 | What is tested automatically, and what must be tested by hand         |
| [`platforms/fedora.md`](platforms/fedora.md)                       | Fedora/Linux data sources, permissions, Wayland and X11               |
| [`platforms/windows.md`](platforms/windows.md)                     | Windows data sources, privileges, packaging caveats                   |
| [`metrics/README.md`](metrics/README.md)                           | The planned metrics engine contract                                   |
| [`widgets/README.md`](widgets/README.md)                           | The planned widget contract                                           |
| [`user-guide/README.md`](user-guide/README.md)                     | End-user documentation (grows with the features)                      |

## Project phases

- **Phase 0 — Foundation** _(current)_: project structure, platform
  abstraction, one real React ↔ Rust command, navigation shell, docs, CI.
- **Phase 1+**: metrics engine, real monitoring, graphs, history, widget engine,
  dashboards, Mini overlay, themes, tray, packaging.

Everything beyond Phase 0 is deliberately out of scope for now; the documents
above describe the _contracts_ those phases must satisfy, not their
implementation.
