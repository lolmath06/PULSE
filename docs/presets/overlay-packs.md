# Overlay packs

Twelve built-in overlays (`src/presets/overlayPacks.ts`), each composed for a
footprint and styled, ready to use — and editable afterwards like any
overlay. Add them from Overlays → _Overlay packs_ (filter by footprint) or
from a mode page.

| Pack                        | Footprint     | Style           | Shows                                                        |
| --------------------------- | ------------- | --------------- | ------------------------------------------------------------ |
| **Tiny Thermals**           | micro         | Compact         | CPU and GPU temperature, RAM — rows                          |
| **Tiny Stats**              | micro         | Compact         | CPU, GPU, RAM, temperature — one slim line                   |
| **Top Bar**                 | top strip     | Clean           | six live trends across the full width at the top             |
| **Bottom Bar**              | bottom strip  | Stealth         | CPU/GPU/RAM load bars and traffic along the bottom edge      |
| **Left Rail**               | left rail     | Neon            | CPU/GPU/RAM rings, temperatures, download — full height      |
| **Right Rail**              | right rail    | Clean           | CPU, RAM, temperature, network and disk charts — full height |
| **Gaming Corner**           | corner HUD    | Gaming          | CPU %, GPU %, CPU °C, GPU °C, VRAM, RAM — 2×3 grid           |
| **Thermal Strip**           | floating card | Neon            | CPU and GPU temperature with trends, SSD — colour-banded     |
| **System Summary Card**     | summary block | Glass           | CPU and RAM rings, GPU group, network group                  |
| **Network Strip**           | floating card | Technical       | download and upload with trends, Wi-Fi signal                |
| **Minimal Transparent HUD** | micro         | Transparent HUD | CPU, GPU, RAM — no panel, haloed figures                     |
| **Dev Monitor Rail**        | right rail    | Technical       | CPU and RAM charts, disk read/write, net, process counts     |

Every figure is a real metric; an unreadable one shows `NAME —` with the
backend's reason. **No FPS, no ping**: PULSE has no real source for either
(tested: no pack, template or Mini layout binds one).

## Footprints

Full-width bars and full-height rails use the overlay `span: 'fill'`: the
overlay keeps the screen's width (or height) and spreads its widgets across
it. Geometry is computed from the primary monitor's logical size; corners sit
24 px from the edges; micro, card and block overlays cascade from the top
left.

## Capability-aware

A pack that needs placement (bars, rails) or stacking (everything that should
stay above a game) carries a hint where the session cannot deliver it — for
example on GNOME with the bridge: _stays above other windows here; drag it to
the edge once in Edit mode — GNOME places Wayland windows itself_. Nothing is
blocked.

## Lifecycle

- An overlay made from a pack remembers its origin (`origin: {pack, version}`)
  and shows _From “…”_ with **Reset to pack** (widgets, chrome and style come
  back; position and name stay).
- **Duplicate** copies any overlay; **Save as my pack** stores its widgets,
  layout, style and chrome in the `templates` section (`overlays`), listed
  under _Your packs_ with Add and Delete.
- `PACKS_VERSION` is recorded with the origin, so a later PULSE can recognise
  overlays made from an older pack.
