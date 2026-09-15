# Recommended labels and milestones

Kept deliberately small. A label that nobody uses is noise.

## Labels

### Type

| Label            | Colour    | Meaning                              |
| ---------------- | --------- | ------------------------------------ |
| `type: bug`      | `#d73a4a` | Something is broken                  |
| `type: feature`  | `#0e8a16` | New capability                       |
| `type: docs`     | `#0075ca` | Documentation only                   |
| `type: refactor` | `#5319e7` | Internal change, no behaviour change |
| `type: chore`    | `#cfd3d7` | Build, CI, tooling                   |

### Platform

Because PULSE treats both platforms as first-class, knowing which one an issue
concerns matters more than usual.

| Label               | Colour    | Meaning                                    |
| ------------------- | --------- | ------------------------------------------ |
| `platform: fedora`  | `#294172` | Fedora / Linux specific                    |
| `platform: windows` | `#00a4ef` | Windows specific                           |
| `platform: both`    | `#7057ff` | Affects both, or needs a decision for both |

### Area

`area: metrics`, `area: widgets`, `area: mini-overlay`, `area: modes`,
`area: theming`, `area: packaging` — all `#fbca04`.

### Status

| Label                               | Colour    | Meaning                                        |
| ----------------------------------- | --------- | ---------------------------------------------- |
| `status: needs triage`              | `#ededed` | Not yet assessed                               |
| `status: needs platform validation` | `#d4c5f9` | Works on one platform, unverified on the other |
| `status: blocked`                   | `#b60205` | Waiting on something else                      |
| `good first issue`                  | `#7057ff` | Small and well-defined                         |

`status: needs platform validation` is the one worth adopting early — it is how
the cross-platform rule stays visible in the issue tracker instead of living
only in `CONTRIBUTING.md`.

## Milestones

One per phase, mirroring the roadmap in `README.md`:

- **Phase 0 — Foundation** _(current)_
- **Phase 1 — Metrics Engine**
- **Phase 2 — Widgets & Dashboards**
- **Phase 3 — Modes & Mini Overlay**
- **Phase 4 — Polish & Distribution**

Phase boundaries are not suggestions: an issue belonging to a later milestone
should wait for it.
