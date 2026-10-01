# Modes

A mode is a starting point for a way of working: a style and density, a
dashboard (from a template), a set of overlay packs, the metrics it shows
first, and a few interaction defaults. Definitions: `src/modes/modes.ts`;
state: the `appearance` section (`activeMode`, `modes`, `mini`).

| Mode            | Style (default) | Density     | Dashboard template    | Suggested packs                                                               | On entering                              |
| --------------- | --------------- | ----------- | --------------------- | ----------------------------------------------------------------------------- | ---------------------------------------- |
| **Gaming**      | Gaming          | compact     | Gaming dashboard      | Gaming Corner, Tiny Stats, Thermal Strip, Top Bar, Minimal HUD, Tiny Thermals | locks every overlay; keeps PULSE running |
| **Development** | Technical       | compact     | Development dashboard | Dev Monitor Rail, Right Rail, Network Strip, Bottom Bar                       | —                                        |
| **Personal**    | Glass           | comfortable | Personal starter      | System Summary Card, Minimal HUD, Tiny Stats, Left Rail                       | —                                        |
| **Mini**        | Compact         | compact     | Minimal clean         | Tiny Stats, Tiny Thermals, Minimal HUD                                        | —                                        |

## How a mode behaves

- **Visiting** a mode page shows it in the mode's style — a preview, nothing
  changes.
- **Entering** a mode makes the whole app wear the mode's style and density
  (unless you chose a density yourself), shows its dashboard, and applies
  its switches (lock overlays, keep running). **Leave mode** returns to your
  chosen style.
- While a mode is on, a style picked in Appearance goes to **that mode**.
- Each mode's page lets you change its style, create its dashboard from its
  template (then edit it like any dashboard), add its packs, and switch its
  behaviours off.

## Pages

Each mode page: hero (name, tagline, Enter/Leave, style), a live strip of the
mode's key metrics, the mode dashboard (read-only there; _Edit in Dashboard_),
its overlay packs, and _When this mode is on_.

Per mode: [`gaming.md`](gaming.md) · [`development.md`](development.md) ·
[`personal.md`](personal.md) · [`mini.md`](mini.md).
