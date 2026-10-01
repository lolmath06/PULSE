# Design system

PULSE's look is one system shared by every surface — the main window, the
dashboard, overlay windows and their previews, the Mini window, the Studio.
It lives in `src/design/` and `src/styles/`:

| File                             | Role                                                                                 |
| -------------------------------- | ------------------------------------------------------------------------------------ |
| `src/design/styles.ts`           | the eight built-in styles, as data                                                   |
| `src/design/appearance.ts`       | the `appearance` configuration section: style, customization, saved styles           |
| `src/design/look.ts`             | style + customization → a resolved **look** → CSS tokens; render-time widget styling |
| `src/design/LookContext.tsx`     | `RootLook` (a whole window), `StyleScope` / `PageStyle` (one part), `LookProvider`   |
| `src/design/hooks.ts`            | `useLook()`, `useAppLook()`, `useScopedLook()`, `withTransition()`                   |
| `src/design/lookContextValue.ts` | the React context object itself                                                      |
| `src/styles/theme.css`           | the token contract and its defaults (Clean), per-style touches tokens cannot express |
| `src/styles/design.css`          | the component layer: controls, dialogs, widgets, overlays, the Studio                |

## Tokens

Every visual decision reads a CSS custom property. A look writes all of them,
to `:root` for a window or to any container for just that part — which is how
a dashboard, an overlay and Mini can each wear a different style at once.

| Family   | Tokens                                                                                                                                                                                                    |
| -------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Canvas   | `--pulse-bg`, `--pulse-backdrop` (gradient layers: aurora, blueprint grid, …)                                                                                                                             |
| Surfaces | `--pulse-surface`, `--pulse-surface-raised`, `--pulse-bg-sunken`, `--pulse-bg-hover` — with translucency                                                                                                  |
| Lines    | `--pulse-border`, `--pulse-border-strong`                                                                                                                                                                 |
| Text     | `--pulse-text`, `--pulse-text-muted`, `--pulse-text-faint`                                                                                                                                                |
| Accent   | `--pulse-accent`, `--pulse-accent-2`, `--pulse-accent-dim`, `--pulse-accent-soft`, `--pulse-accent-contrast`                                                                                              |
| Data     | `--pulse-viz-1…6` (series, in order), `--pulse-viz-grid`                                                                                                                                                  |
| Semantic | `--pulse-success-text`, `--pulse-warning-*`, `--pulse-danger-*` (the same in every style)                                                                                                                 |
| Type     | `--pulse-font`, `--pulse-font-display`, `--pulse-font-numeric`, `--pulse-font-mono`, `--pulse-font-scale`, `--pulse-weight-title`, `--pulse-weight-value`, `--pulse-title-case`, `--pulse-title-tracking` |
| Shape    | `--pulse-radius-sm`, `--pulse-radius`, `--pulse-radius-lg`, `--pulse-radius-xl`, `--pulse-chamfer`                                                                                                        |
| Depth    | `--pulse-shadow`, `--pulse-shadow-hover`, `--pulse-shadow-pop`, `--pulse-highlight`, `--pulse-blur`                                                                                                       |
| Light    | `--pulse-glow`, `--pulse-glow-color`, `--pulse-line-glow`, `--pulse-value-shadow`                                                                                                                         |
| Space    | `--pulse-space-1…6` = 4 8 12 16 24 32 px × density                                                                                                                                                        |
| Motion   | `--pulse-motion-fast` (120 ms), `--pulse-motion` (200 ms), `--pulse-motion-slow` (380 ms), `--pulse-ease`, `--pulse-ease-spring`                                                                          |

Readability is tested: every style's text has ≥ 7:1 contrast on its canvas and
muted text ≥ 4.5:1 (`src/design/design.test.tsx`).

## Hierarchy

- **Page**: eyebrow (accent, tracked caps) → title (display family,
  28 px × scale) → subtitle (muted, ≤ 68 ch).
- **Card**: title in small tracked capitals (or sentence case, per style),
  then content; notes in faint text.
- **Widget**: an accent tick and the title in the style's title case; the
  number carries the style's value weight and numeric family; units at 0.5–0.6
  of the number, muted. When a card's title already names a single-metric
  chart, the chart does not repeat it.
- **Micro widgets** (`CPU 31 %`): label muted, value strong, unit smaller;
  the label goes first when space runs out, then the unit, never the value.
  Text is fitted with a 0.62 em glyph budget, measured on the fallback fonts
  PULSE meets, so nothing ends in "…" by surprise. _Labels in tiny widgets_
  can be switched off globally.

## Density

`compact` (× 0.78), `comfortable` (× 1), `spacious` (× 1.22) scale the space
tokens, the default widget padding and the dashboard gap. Grid units never
change: a layout is the same layout at every density.

## Motion

Short, eased and purposeful: page entrance (rise + fade), hover elevation of
widgets and style cards, dialogs and menus popping in, the drawer sliding,
switches with a spring, the Edit outline of an overlay breathing, style and
mode changes cross-faded with the View Transitions API where the webview has
it. Everything uses the motion tokens, so **Motion: None** (or the system's
_reduce motion_) turns all of it off.

## Styles

| Style               | Identity                                                                     | Best for             |
| ------------------- | ---------------------------------------------------------------------------- | -------------------- |
| **Clean**           | charcoal, soft depth, one teal accent — the default                          | dashboard, Mini      |
| **Glass**           | frosted translucent panels over an aurora, top-edge highlight, 18 px corners | dashboard (showcase) |
| **Technical**       | blueprint grid, square edges, monospaced amber readouts, markers and axes    | dashboard, overlay   |
| **Neon**            | near-black, magenta and cyan light, glowing lines and edges                  | dashboard, overlay   |
| **Gaming**          | chamfered corners, crimson and amber, condensed heavy numbers                | overlay, dashboard   |
| **Stealth**         | almost no chrome, muted slate, no fills or grids                             | overlay, Mini        |
| **Compact**         | dense spacing, smaller type, quiet charts                                    | Mini, overlay        |
| **Transparent HUD** | no panels at all; bright figures with a dark halo                            | overlay              |

A style is a complete look — canvas, surfaces and translucency, depth, glow,
shape, families and weights, density, the chart treatment, the overlay
chrome — not a palette.

## How a style reaches a widget

At render time, never in storage (`styleWidget`):

- a chart still **on its preset** takes the style's treatment (line, fill,
  grid, axes, which parts show) and the user's global chart tuning; its
  renderer, size, scale, precision, thresholds and series names stay its own;
- a chart the user **customised** keeps exactly what they chose;
- an **untouched frame** takes the style's radius, padding and border;
- titles and micro labels follow the global switches; the text size follows
  the global font scale.

Theme-coloured charts read `--pulse-viz-*`, so they recolour with no change.

## Surfaces and scopes

| Surface         | Wears                                                        |
| --------------- | ------------------------------------------------------------ |
| Main window     | the app style (`appearance.styleId` + customization)         |
| A dashboard     | its own `styleId`, or the app's (Dashboard header → _Style_) |
| An overlay      | its own `styleId`, or the app's (overlay editor → _Style_)   |
| Mini            | the app style                                                |
| Studio previews | each style card and the live preview wear their own look     |

Choosing a style for an overlay writes that style's chrome (background,
opacity, border, shadow, radius, padding) and gap into the overlay **once**,
then refits the window: an overlay's size never changes behind it, and the
chrome stays tunable. `backdrop-filter` blur cannot see the desktop through a
transparent window, so overlays get translucency and edges, not real frost.

Customization is documented for users in
[`customization.md`](customization.md).
