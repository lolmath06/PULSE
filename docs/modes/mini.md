# Mini mode

**A small, elegant monitor window.** Mini is an ordinary window — movable,
focusable, in the taskbar, never always-on-top and never click-through (for
that, use an overlay).

- **Mini layouts** (`src/modes/miniLayouts.ts`), sized for a narrow window:
  _Vitals_ (CPU, RAM, temperature, traffic with trends), _Thermals_
  (temperature chart and strip), _Network_ (download and upload charts, Wi-Fi),
  _Focus_ (one big CPU ring, memory and heat meters). Or any dashboard,
  stacked in one column.
- **Style**: the Mini mode's (Compact by default), or its own.
- Mini → _Open Mini window_; the window's selector switches layout or
  dashboard at once, and it follows style changes live.
- The Mini page previews the chosen layout live, in a Mini-sized frame.
