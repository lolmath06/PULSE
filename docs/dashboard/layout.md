# Dashboard — Layout

`src/dashboard/layout.ts` — pure functions, all tested.

## Grid

- **12 columns**, rows of 44 px, 10 px gaps. Positions are cells `{ x, y, w, h }`,
  never pixels: the same layout is right at any resolution or scale.
- Per-kind limits:

  | Kind          | min w×h | max w×h | default |
  | ------------- | ------- | ------- | ------- |
  | visualization | 2×2     | 12×12   | 4×4     |
  | value         | 1×1     | 6×4     | 2×2     |
  | group         | 2×2     | 12×8    | 3×3     |
  | summary       | 2×1     | 12×4    | 6×2     |

- Widgets receive their **exact** content box (cell size minus frame and
  title) and pass it to `MetricVisualization`; resizing is live, no reload.

## Keeping the layout valid

- `clampRect` — inside the grid and the kind's limits.
- `moveWidget` / `resizeWidget` — the moved widget stays where it was put; any
  widget it lands on is pushed down; then everything floats up (`compact`).
- `firstFit` — where a new widget goes.
- `repairLayout` — applied on every load: a hand-edited or older file can
  never leave overlaps (idempotent, tested).

## Editing

- **Locked** (default): nothing moves; charts are fully interactive.
- **Edit**: each widget shows an explicit **drag handle** (⠿), actions
  (Customize, Duplicate, To overlay, ×) and a resize grip. The chart surface
  is never a drag target, so its tooltip keeps working.
- While dragging, other widgets make room live using the same pure functions;
  the change is saved on release only.
- **Keyboard:** focus a widget (Tab), then arrows move it, Shift + arrows
  resize it, Delete removes it, Enter opens Customize.

## Responsive

The grid measures its container: ≥ 880 px 12 columns, ≥ 540 px 6 columns
(x and widths halved, then repaired), below that one column in reading order.
Narrow layouts are **display-only** — the stored 12-column layout is never
rewritten by a narrow window — and editing asks for a wider window.
