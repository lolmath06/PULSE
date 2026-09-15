# Features

Self-contained feature slices: a feature's UI, its state and its logic live
together here rather than being scattered across `components/`, `hooks/` and
`stores/`.

Empty in Phase 0. The first occupants will be the metrics display, the widget
engine and the dashboard editor.

A feature slice may depend on `components/`, `hooks/`, `services/` and `utils/`.
It must not depend on another feature slice — shared code moves up a level.
