# Customization

PULSE → **Appearance** (Studio). Every control either **follows the style**
(marked _Style_) or holds your own value; _Use style_ puts it back, and
_Reset every tweak_ clears them all. Switching style keeps your own values.

| Group            | Controls                                                                                                                    |
| ---------------- | --------------------------------------------------------------------------------------------------------------------------- |
| Colour           | accent (10 presets, the style's own, or any colour), chart palette (the style's, Aurora, Ocean, Ember, Forest, Candy, Mono) |
| Surfaces         | panel opacity, glass blur, border strength, shadow depth                                                                    |
| Shape & type     | corner roundness, font (the style's, Sans, Rounded, Condensed, Mono), text size                                             |
| Layout & density | density, widget padding, gap between widgets, overlay padding and gap (used when an overlay takes a style)                  |
| Charts           | line thickness, fill opacity, line shape — for charts still on their preset                                                 |
| Content & motion | widget titles, labels in tiny widgets, motion (full, reduced, none)                                                         |

A chosen accent also leads the chart palette, so single-series charts wear it.
Fonts are stacks of common system families (Inter, Cantarell, Segoe UI,
Nunito, Barlow Condensed, JetBrains Mono, …); a family that is not installed
falls back to the next one. PULSE ships no font files.

## Your styles

**Save current look** stores the current style plus your tuning under a name.
Saved styles can be used, renamed, duplicated, deleted, **exported** (a small
JSON: `{"format": "pulse.style", "version": 1, "style": {…}}`) and
**imported**. Tuning a saved style detaches it: the saved copy is unchanged
until you save again.

## Per-surface styles

A dashboard and an overlay can each wear a style of their own (the _Style_
selector in the dashboard header and in the overlay editor); _App style_
follows whatever the app wears.

## Stored as

The `appearance` section of `ui-config.json`, normalised field by field on
every read: unknown fields are dropped, invalid ones fall back alone, numbers
are bounded (`src/design/appearance.ts`).
