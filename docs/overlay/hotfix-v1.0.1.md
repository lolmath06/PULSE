# v1.0.1 overlay and native-titlebar hotfix

## Diagnosis

- **Overlay layout:** the edit bar used absolute positioning over the metric
  boxes. WebKitGTK measured Tiny Stats' metric at `y=4..34` and its bar at
  `y=2..28`. The inline-flex surface also expanded past the native viewport
  (368 px in a 360 px window), putting the resize grip beyond its edge.
  Existing box-layout tests could not detect either browser layout defect.
- **Locked controls:** React already unmounted controls for `locked=true`.
  Two mechanically reproduced configuration races instead left the frontend
  in Edit while the backend was Locked: loading before installing the event
  listener lost a concurrent lock; accepting an older event after a newer
  lock restored stale Edit state. Listening first, buffering during load and
  checking section revisions closes these paths. This does not establish that
  every reported physical rendering artifact came from one of these races.
- **Main close button:** the main window uses native decorations; its X is
  outside the webview. There is no React titlebar, CSS draggable region,
  `pointer-events` rule, DOM overlay or CSS stacking context over that button.
  Locked Tao 0.35.3's `WlHeader` wraps `GtkHeaderBar` in an above-child
  `GtkEventBox`, adding an intercepting input window. GTK inspection confirms
  that lowering the EventBox removes that extra input window. This matches
  [the upstream CSD issue and fix](https://github.com/tauri-apps/tao/pull/1218)
  and [GTK's documented input ordering](https://docs.gtk.org/gtk3/method.EventBox.set_above_child.html).

## Scope and behavior

Edit controls occupy separate layout rows; horizontal layouts wrap and grids
reduce their column count as the viewport narrows. Widgets retain their pixel
sizes. Native minimum dimensions prevent shrinking below the resulting
content and control rows; a temporary scroll viewport keeps controls reachable
while the compositor applies the constraint. A single widget must be reduced
in its editor to make the window narrower than that widget. Preview uses the
stored window width. Fit to widgets still requests the natural composition;
native minimum height also accounts for edit controls.

Locked removes the bar, its buttons, the drag regions and the resize row;
the remaining surface is inert. Native click-through, focus and always-on-top
policies are unchanged. Configuration listening remains event driven.

Titlebar compatibility is limited to the main window and the exact
EventBox/HeaderBar structure found in the locked Tao. Windows and other
native titlebar structures are untouched. The adjustment runs at setup and
reopening/recreation, without resize toggles or global GTK changes. It can be
removed when the locked Tao no longer creates that structure. No dependency
upgrade, release metadata change, polling or recurring animation is included.

## Repeatable checks

Standard checks from the repository root:

```sh
pnpm typecheck
pnpm lint
pnpm format:check
pnpm test --maxWorkers=2
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo test --locked --manifest-path src-tauri/Cargo.toml
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
pnpm rust:windows:lint
git diff --check
sh scripts/check-git-attribution.sh --all
sh scripts/check-case-collisions.sh
```

The targeted frontend tests are in `src/overlay/overlay.test.tsx` and
`src/config/uiConfig.test.ts`. They include narrow heterogeneous layouts,
fill layouts, Edit/Locked/Edit, absence of locked interactive DOM, minimum-size
feedback, startup locking and event ordering across sections.

With GTK 3, WebKitGTK 4.1, PyGObject, pycairo, Pillow and an available display,
run the isolated native-renderer probe using PULSE's startup selection:

```sh
cargo run --locked --manifest-path src-tauri/Cargo.toml --example overlay_webkit_probe -- --serve
cargo test --locked --manifest-path src-tauri/Cargo.toml window_native::tests -- --ignored --test-threads=1
```

The WebKit probe uses the real frontend and an ephemeral showcase backend,
starts/stops only its own Vite child, and writes evidence under
`/tmp/pulse-overlay-webkit` (override with `--output`). Remove manually exported
renderer overrides when testing automatic selection; in particular the old
`WEBKIT_DISABLE_DMABUF_RENDERER=1` intentionally reproduces the bug.
The original 93 states in English/French at requested widths 200, 360 and
900 px, including heterogeneous grids, remain covered. It checks actual rectangles and hit
locations, native GTK minimum sizes, lock/unlock, Open PULSE and resize command
dispatch, and absence of minimum-size calls during idle intervals. The GTK
titlebar test checks the real GDK input-window removal and preservation through
hide/show/resize; it is opt-in because ordinary CI has no display. Pixel
comparisons and additional states are described below.

These probes do not replace end-to-end physical clicks through Mutter or
Windows/WebView2. The native size adapter in the WebKit probe applies GTK
hints directly; Tauri's command dispatch is covered separately by build and
frontend tests. CPU idle has no new polling/animation source, but has not been
benchmarked on both physical platforms.

## Transparent repaint follow-up

### Proven cause

Reproduced on Fedora 39, GNOME Wayland, nouveau, GTK 3.24.43 and
WebKitGTK 2.46.3. Two different images of the **same state** distinguish the
DOM from the displayed backing store:

1. Inspect the DOM, including all WidgetCards and Edit controls, their ids,
   rectangles and computed backgrounds/compositing properties.
2. Capture the actual GDK window **before** asking WebKit for a fresh snapshot.
3. Compare that native surface with WebKit's fresh transparent snapshot.
4. Unmount the entire overlay: the expected alpha is exactly zero everywhere.

With the old renderer, Locked has zero Edit controls in the DOM, but the
native image retains the bar and previous readings. After complete unmount,
WebKit's snapshot is empty and the native buffer still contains old pixels.
There is one overlay tree, one card per configured widget, unique React keys
(`key={box.id}`), and retained node identity across layout changes. Removed
widgets disconnect; no old React tree explains the images.

This is the software **UI-process WebKit backing store**, rather than an
overlay layout error. In WebKitGTK 2.46.3,
`BackingStore::incorporateUpdate` requests `CompositeOperator::Copy`, but
`ShareableBitmapSkia::paint` passes a null `SkPaint` to `drawImageRect`, which
uses `SrcOver`. Transparent updates blend into a persistent old image instead
of replacing it. See the exact [upstream correction, 317608](https://commits.webkit.org/317608@main)
and [the affected source](https://github.com/WebKit/WebKit/blob/webkitgtk-2.46.3/Source/WebCore/platform/graphics/skia/ShareableBitmapSkia.cpp).
That explains both persistent old text and increasing background opacity.

`html`, `body`, `#root` and the overlay are transparent. The separate chrome
span has the requested partial opacity. Overlay widgets disable the dashboard
backdrop filter; the live surface has no transform, paint containment or
`will-change`. Isolation establishes stacking contexts but does not clear
WebKit's backing store. Tao already clears the native window with Cairo
`SOURCE`. An extra clear before WebView drawing, GTK-managed transparent
background and disabling WebKit acceleration were tested and did not fix the
corrupted image upstream of GTK drawing. No such workaround was retained.

### Correction and scope

PULSE's automatic nouveau workaround used to disable the DMA-BUF renderer
entirely, selecting this defective software backing-store path. Startup now
selects **software GL plus shared-memory compositor transport** with
`LIBGL_ALWAYS_SOFTWARE=1` and `WEBKIT_DMABUF_RENDERER_FORCE_SHM=1`. Shared-memory
transport alone did not reliably present updates on this nouveau machine;
the combination does. It avoids hardware DMA-BUF without disabling WebKit's
compositor and keeps alpha, transparent holes, chrome opacity and widget
rendering. It affects only PULSE and its children, before GTK initialization.
Explicit user renderer choices remain untouched; other drivers are unchanged.
Software composition is a performance tradeoff on nouveau, not a transparency
fallback. Its CPU cost has not been benchmarked.

No production CSS, DOM, timer, animation, native input policy or titlebar code
is changed by this follow-up. The earlier layout/configuration/titlebar fixes
addressed independent defects. Their renderer test was opaque by default and
only checked DOM geometry, so it could not catch a corrupt transparent buffer.

### Regression evidence and limits

The probe now checks 104 DOM states and 105 native/WebKit image pairs: the
original pack/locale/resize matrix, stable React keys across three layouts,
metric updates, background removal, widget removal and full unmount.
`report.json` retains counts, rectangles, computed styles and PNG paths.
Failures distinguish `DOM_DUPLICATE` from `STALE_NATIVE_PAINT`. A deliberate
duplicate (`--inject-duplicate`) must exit 1 before any pixel pass.

The pixel oracle checks all locally flat reference pixels, including vacated
text/control locations and transparent holes. CPU snapshots and GL display
rasterization differ at glyph/path edges and rounded corners; these regions
are recorded separately. Premultiplied color tolerance is 6/255 and alpha
tolerance 2/255; there is no allowed percentage of bad pixels. Full unmount
requires **exactly zero alpha everywhere**, without exclusions or tolerance.
Test-only fixed metrics and disabled CSS transitions keep comparisons stable.
At most three 200 ms retries observe natural presentations during an
asynchronous native resize. Every attempt is saved; the test never requests a
repaint or clears a buffer to make an assertion pass.

The GTK adapter applies the same widget-level input shape as PULSE. A
`WAYLAND_DEBUG=client` run verifies `wl_surface.set_input_region` receives an
empty region in Locked and `nil` in Edit through the resize sequence. This
checks compositor protocol requests; a physical click through Mutter remains
part of the short final check, not an automated claim.

### Recorded results (Fedora 39 / WebKitGTK 2.46.3)

| Check                                                          | Result                                                                              |
| -------------------------------------------------------------- | ----------------------------------------------------------------------------------- |
| Old renderer, `WEBKIT_DISABLE_DMABUF_RENDERER=1`               | 104 DOM states pass; 78/105 surface comparisons fail, exit 1 (`STALE_NATIVE_PAINT`) |
| PULSE automatic nouveau renderer                               | 104 DOM states and 105/105 surface comparisons pass, exit 0                         |
| Entire overlay unmounted                                       | Old: 490,000 nontransparent pixels remain. Fixed: all 490,000 pixels have alpha 0   |
| Bare Locked surface                                            | 487,005 fully transparent pixels; current widgets preserved                         |
| Injected duplicate WidgetCard                                  | Exit 1 with `DOM_DUPLICATE`, before pixel validation                                |
| Locked DOM                                                     | All 35 Locked states contain zero Edit controls                                     |
| Wayland trace                                                  | 35 empty input regions, 68 interactive resets, no nonempty Locked region            |
| Frontend suite                                                 | 43 files, 890 tests pass                                                            |
| Rust suite                                                     | 1,683 pass, 2 display/environment tests ignored by default                          |
| Explicit GTK titlebar test                                     | 1 passes; input-window fix unchanged                                                |
| Typecheck, ESLint, Clippy (all targets), Rust formatting       | Pass                                                                                |
| Frontend and native debug builds                               | Pass; existing Vite large-chunk warning                                             |
| Changed Markdown formatting, Python syntax, `git diff --check` | Pass                                                                                |

The full Rust suite initially had 9 sandbox failures involving netlink and
kernel PID visibility; rerunning with host access passed all 1,683 tests.
Process-control tests only act on their own spawned children. The native
pixel test required one extra natural presentation for one of its 105 fixed
states; the old renderer still failed after all bounded retries.

Local evidence from this run: `/tmp/pulse-overlay-verified/report.json`,
`/tmp/pulse-overlay-old-verified/report.json`,
`/tmp/pulse-overlay-duplicate/report.json` and
`/tmp/pulse-overlay-verified-wayland.log`. Every report links its native,
WebKit and difference images, including retry attempts.

To repeat the two negative controls (both should exit 1 on this affected
WebKit version):

```sh
WEBKIT_DISABLE_DMABUF_RENDERER=1 cargo run --locked --manifest-path src-tauri/Cargo.toml --example overlay_webkit_probe -- --serve --output /tmp/pulse-overlay-old
cargo run --locked --manifest-path src-tauri/Cargo.toml --example overlay_webkit_probe -- --serve --inject-duplicate --output /tmp/pulse-overlay-duplicate
```

## Remaining physical Fedora check

Using a newly started build with no old renderer override: open Tiny Stats,
resize wide/narrow three times, then Edit → Locked → Edit. No old reading or
bar should remain. In Locked click an application through the overlay. Finish
with one click on the main X (already physically validated before this change).
