# Dashboard templates

Eight built-in dashboards (`src/presets/dashboardTemplates.ts`), composed on
the 12-column grid (tested: nothing overlaps, nothing leaves the grid). Use
them from Dashboard → **New** (a gallery with a live-style schematic of each),
from a mode page, or in the welcome.

| Template                  | Style     | For         | Layout                                                                                   |
| ------------------------- | --------- | ----------- | ---------------------------------------------------------------------------------------- |
| **Default balanced**      | app style | everyone    | summary strip; CPU and memory; thermals, GPU, network; storage                           |
| **Gaming dashboard**      | Gaming    | Gaming      | summary; CPU and GPU rings; temperature chart; memory, video memory, network             |
| **Development dashboard** | Technical | Development | summary; CPU chart; process counts; memory, disk I/O, network; CPU temp; filesystem      |
| **Personal starter**      | Glass     | Personal    | summary; processor chart; memory and temperature rings; network; storage                 |
| **Thermal focus**         | Neon      | Gaming      | temperature chart with bands; CPU package ring; four temperature tiles; load             |
| **Network & I/O**         | Technical | Development | network and disk charts; ↓ ↑ Wi-Fi SSD tiles; filesystem                                 |
| **Minimal clean**         | Stealth   | Mini        | four big numbers and one quiet chart                                                     |
| **Fancy showcase** ★      | Glass     | Personal    | live strip with trends; processor chart; memory, graphics and heat rings; network; tiles |

- A dashboard from a template wears the template's style (change it with the
  header's _Style_) and remembers its origin; **Reset** returns to the
  template (widgets and style).
- Temperature rings use a fixed 20–100 °C visual range; temperature values
  and charts use visual colour bands (70/85 °C) — a look, not a verdict.
- User side: duplicate, rename, export/import (`pulse.dashboard` JSON) as
  before; widget templates (_Save as template_) as before.
