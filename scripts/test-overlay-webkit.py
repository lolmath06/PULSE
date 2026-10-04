#!/usr/bin/env python3
"""Real WebKitGTK overlay regression test against a running PULSE Vite server.

Usage: python3 scripts/test-overlay-webkit.py [http://127.0.0.1:4318]
Requires GTK 3 / WebKit2 4.1 PyGObject, Pillow, pycairo and a display. Uses an ephemeral web
context and the showcase backend: no real PULSE configuration or metrics.
Only the test's own window is resized/closed; no global pointer injection.
Writes per-state DOM counts, boxes, computed styles, native/WebKit PNG pairs,
diffs and report.json. A clean DOM is NOT a pixel pass. Exit 1 on either failure.
Prefer cargo run --manifest-path src-tauri/Cargo.toml --example overlay_webkit_probe
-- --serve to use PULSE's real renderer selection (do not export old overrides).
"""

import argparse
import atexit
import json
import os
import pathlib
import subprocess
import sys
import time
import urllib.parse
import urllib.request

import cairo
from PIL import Image, ImageChops, ImageFilter

import gi

gi.require_version("Gtk", "3.0")
gi.require_version("Gdk", "3.0")
gi.require_version("WebKit2", "4.1")
from gi.repository import Gdk, GLib, Gtk, WebKit2  # noqa: E402

ROOT = pathlib.Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('base', nargs='?', default='http://127.0.0.1:4318')
parser.add_argument('--serve', action='store_true', help='own a temporary Vite child at the base URL')
parser.add_argument('--output', type=pathlib.Path, default=pathlib.Path('/tmp/pulse-overlay-webkit'))
parser.add_argument('--inject-duplicate', action='store_true', help='negative control: duplicate a WidgetCard')
args = parser.parse_args()
BASE = args.base.rstrip('/')
args.output.mkdir(parents=True, exist_ok=True)
server = None


def stop_server():
    if server is not None and server.poll() is None:
        server.terminate()
        server.wait()


atexit.register(stop_server)
if not Gtk.init_check()[0]:
    raise RuntimeError('A GTK display is required')
if args.serve:
    url = urllib.parse.urlsplit(BASE)
    if url.scheme != 'http' or url.hostname != '127.0.0.1' or url.port is None:
        parser.error('--serve requires http://127.0.0.1:<port>')
    server = subprocess.Popen([
        'node', str(ROOT / 'node_modules/vite/bin/vite.js'),
        '--host', '127.0.0.1', '--port', str(url.port), '--strictPort',
    ], cwd=ROOT, stdout=subprocess.DEVNULL)
    try:
        for _ in range(100):
            if server.poll() is not None:
                raise RuntimeError('The test Vite server exited (port already used?)')
            try:
                urllib.request.urlopen(BASE, timeout=1).close()
                break
            except OSError:
                time.sleep(.05)
        else:
            raise RuntimeError('The test Vite server did not start')
    except BaseException:
        stop_server()
        raise

fixture = json.loads((ROOT / "scripts/showcase/fixture/catalog.json").read_text())
# Freeze fixture samples between captures, including live ticks. Advancing this
# clock below exercises changing CPU/GPU/RAM/temperature without screenshot races.
bootstrap = "window.probeTime = 1800000000000; Date.now = () => window.probeTime;"
bootstrap += "window.__PULSE_SHOWCASE__ = " + json.dumps(fixture) + ";"
bootstrap += (ROOT / "scripts/showcase/fixture/backend.js").read_text()
bootstrap += r"""
window.pulseProbe = message => window.webkit.messageHandlers.pulseProbe.postMessage(JSON.stringify(message));
window.addEventListener('error', event => pulseProbe({kind:'error', message:event.message, stack:event.error?.stack}));
window.probeCalls = { minimum: 0, open: 0, resize: 0 };
const originalInvoke = window.__TAURI_INTERNALS__.invoke;
window.__TAURI_INTERNALS__.invoke = (command, args) => {
  if (command === 'plugin:window|set_min_size') {
    window.probeCalls.minimum++;
    pulseProbe({ kind: 'minimum', ...JSON.parse(JSON.stringify(args.value)) });
    return Promise.resolve();
  }
  if (command === 'open_main_window') window.probeCalls.open++;
  if (command === 'plugin:window|start_resize_dragging') {
    window.probeCalls.resize++;
    return Promise.resolve();
  }
  return originalInvoke(command, args);
};
"""

RUNNER = r"""
(async () => {
  const { createOverlayFromPack, OVERLAY_PACKS } = await import('/src/presets/overlayPacks.ts');
  // Use the same singleton as main, including Vite's HMR revision query.
  const main = await (await fetch(document.querySelector('script[src*="/src/main.tsx"]').src)).text();
  const configPath = main.match(/from ["']([^"']*config\/uiConfig\.ts[^"']*)["']/)[1];
  const { writeSection, readSection } = await import(configPath);
  while (!document.documentElement.classList.contains('pulse-overlay')) await new Promise(resolve => setTimeout(resolve, 25));
  const { setActiveLocale } = await import('/src/i18n/i18n.ts');
  const { overlayLayout } = await import('/src/overlay/model.ts');
  const pixelStyle = document.createElement('style');
  pixelStyle.textContent = '* { transition: none !important; animation: none !important; }';
  document.head.append(pixelStyle);
  const pause = ms => new Promise(resolve => setTimeout(resolve, ms));
  const assert = (condition, message) => { if (!condition) throw new Error(message); };
  const intersects = (a, b) => a.left < b.right && b.left < a.right && a.top < b.bottom && b.top < a.bottom;
  const capture = (name, empty = false) => new Promise(resolve => {
    window.probeCaptured = resolve;
    const selectors = ['html','body','#root','.overlay','.overlay__backdrop','.overlay__viewport',
      '.overlay__content','.overlay__slot','.widget','.viz','.overlay__header','.overlay__footer'];
    const styles = selectors.flatMap(selector => [...document.querySelectorAll(selector)].map(el => {
      const css = getComputedStyle(el);
      return {selector, box:el.getBoundingClientRect().toJSON(), background:css.background,
        opacity:css.opacity, backdropFilter:css.backdropFilter, transform:css.transform,
        contain:css.contain, isolation:css.isolation, willChange:css.willChange};
    }));
    // GPU compositing and the CPU WebKit snapshot rasterize rounded corners
    // differently. Exclude only their small corner squares, never text/controls.
    const corners = [...document.querySelectorAll('.overlay, .overlay__backdrop, .widget, .overlay__bar, button')]
      .flatMap(el => {
        const r = el.getBoundingClientRect(), css = getComputedStyle(el);
        return [['TopLeft',r.left,r.top,1,1],['TopRight',r.right,r.top,-1,1],
                ['BottomLeft',r.left,r.bottom,1,-1],['BottomRight',r.right,r.bottom,-1,-1]]
          .flatMap(([corner,x,y,sx,sy]) => {
            const radius = Math.ceil(parseFloat(css['border'+corner+'Radius']));
            return radius ? [{x:x+(sx<0?-radius:0),y:y+(sy<0?-radius:0),width:radius,height:radius}] : [];
          });
      });
    pulseProbe({kind:'capture', name, empty, editing:!!document.querySelector('.overlay--editing'),
      viewport:[innerWidth,innerHeight], styles, corners,
      cards:[...document.querySelectorAll('.widget')].map(el=>({id:el.dataset.widgetId,box:el.getBoundingClientRect().toJSON(),text:el.textContent})),
      controls:[...document.querySelectorAll('.overlay__button,.overlay__bar,.overlay__resize')].map(el=>({className:el.className,box:el.getBoundingClientRect().toJSON()}))});
  });
  let context = '';
  let cases = 0;
  function check(editing) {
    const root = document.querySelector('.overlay');
    assert(root, 'Missing overlay: ' + JSON.stringify({body:document.body.innerHTML, overlays:readSection('overlays'), pulse:document.documentElement.className}));
    const expected = readSection('overlays').items.find(o=>o.id==='o-probe');
    const widgets = [...document.querySelectorAll('.widget')];
    const ids = widgets.map(el=>el.dataset.widgetId);
    assert(document.querySelectorAll('.overlay').length === 1, context + ': DOM_DUPLICATE overlay tree');
    assert(widgets.length === expected.widgets.length && new Set(ids).size === ids.length &&
      expected.widgets.every(w=>ids.includes(w.id)), context + ': DOM_DUPLICATE WidgetCard ' + JSON.stringify(ids));
    assert(document.querySelectorAll('.overlay__bar').length === (editing ? 1 : 0) &&
      document.querySelectorAll('.overlay__button').length === (editing ? 2 : 0) &&
      document.querySelectorAll('.overlay__resize').length === (editing ? 1 : 0), context + ': DOM_DUPLICATE edit controls');
    const slots = [...root.querySelectorAll('.overlay__slot')];
    const cards = slots.map(e => e.getBoundingClientRect());
    const controls = [...root.querySelectorAll('.overlay__bar, .overlay__resize')];
    assert(root.getBoundingClientRect().width <= innerWidth, context + ': surface exceeds window');
    for (let i = 0; i < cards.length; i++) {
      const a = cards[i];
      assert(a.left >= 0 && a.top >= 0 && a.right <= innerWidth + 1 && a.bottom <= innerHeight + 1,
        context + ': card outside viewport ' + JSON.stringify({card:a.toJSON(), viewport:[innerWidth,innerHeight]}));
      for (const b of cards.slice(i + 1)) assert(!intersects(a, b), context + ': overlapping cards');
      for (const control of controls) assert(!intersects(a, control.getBoundingClientRect()), context + ': control covers card');
      assert(getComputedStyle(slots[i]).overflow === 'hidden', context + ': uncontained widget paint');
    }
    if (editing) {
      assert(!root.inert, context + ': Edit is inert');
      for (const button of root.querySelectorAll('button')) {
        const r = button.getBoundingClientRect();
        assert(r.left >= 0 && r.right <= innerWidth && r.bottom <= innerHeight, context + ': clipped button');
        assert(document.elementFromPoint(r.x + r.width / 2, r.y + r.height / 2) === button,
          context + ': covered button');
      }
      const grip = root.querySelector('.overlay__resize');
      const r = grip.getBoundingClientRect();
      assert(document.elementFromPoint(r.x + r.width / 2, r.y + r.height / 2) === grip, context + ': covered resize grip');
    } else {
      assert(root.inert, context + ': Locked is interactive');
      assert(!root.querySelector('button,[tabindex],[data-tauri-drag-region],.overlay__resize,.overlay__header,.overlay__footer'),
        context + ': edit control remains in Locked DOM');
    }
    cases++;
  }
  const names = ['tiny-stats', 'tiny-thermals', 'top-bar', 'left-rail', 'gaming-corner'];
  const packs = OVERLAY_PACKS.filter(pack => names.includes(pack.id));
  assert(packs.length === names.length, 'Missing regression pack');
  for (const locale of ['en', 'fr']) {
    setActiveLocale(locale);
    for (const pack of packs) {
      const created = createOverlayFromPack({version:1,items:[]}, pack, {width:900,height:700});
      const overlay = {...created.section.items[0], id:'o-probe'};
      for (const width of [200, 360, 900]) {
        context = `${locale}/${pack.id}/${width}`;
        writeSection('overlays', {version:1, items:[overlay]});
        await pause(500);
        pulseProbe({kind:'resize', width, height:180});
        await pause(500);
        if (window.probeInjectDuplicate) {
          const card = document.querySelector('.widget');
          card.parentElement.append(card.cloneNode(true));
        }
        check(true);
        await capture(context+'-edit');
        const open = document.querySelectorAll('.overlay__button')[1];
        const beforeOpen = window.probeCalls.open;
        open.click();
        assert(window.probeCalls.open === beforeOpen + 1, context + ': Open PULSE not dispatched');
        const beforeResize = window.probeCalls.resize;
        document.querySelector('.overlay__resize').dispatchEvent(new PointerEvent('pointerdown', {bubbles:true,button:0}));
        assert(window.probeCalls.resize === beforeResize + 1, context + ': resize not dispatched');
        document.querySelector('.overlay__button').click();
        await pause(500);
        assert(readSection('overlays').items[0].locked, context + ': Lock not stored');
        check(false);
        await capture(context+'-locked');
        const section = {version:1,items:[overlay]};
        await window.__TAURI_INTERNALS__.invoke('plugin:event|emit', {
          event:'ui-config-changed', payload:{revision:1000 + cases,section:'overlays',origin:'backend',value:section}
        });
        await pause(500);
        check(true);
        await capture(context+'-edit-again');
        const count = window.probeCalls.minimum;
        await pause(500);
        assert(window.probeCalls.minimum === count, context + ': idle minimum-size loop');
      }
    }
  }
  // Heterogeneous grid: exercise a column-count change in the actual renderer.
  const overlay = {...createOverlayFromPack({version:1,items:[]}, packs.find(p=>p.id==='gaming-corner')).section.items[0],id:'o-probe',layout:'grid',columns:3};
  for (const width of [220,450,900]) {
    context = `grid/${width}`;
    writeSection('overlays',{version:1,items:[overlay]});
    pulseProbe({kind:'resize',width,height:300});
    await pause(500);
    check(true);
    await capture(context);
    const boxes = overlayLayout(overlay, innerWidth).boxes;
    assert(boxes.length === document.querySelectorAll('.overlay__slot').length, 'grid slots missing');
  }
  // Keep React keys while moving the same widgets through different layouts.
  const retained = new Map([...document.querySelectorAll('.widget')].map(el=>[el.dataset.widgetId,el]));
  for (const layout of ['vertical','horizontal','grid']) {
    for (const locked of [true,false]) {
      context = `same-keys/${layout}/${locked?'locked':'edit'}`;
      writeSection('overlays',{version:1,items:[{...overlay,layout,locked}]});
      pulseProbe({kind:'resize',width:700,height:700});
      await pause(500);
      check(!locked);
      for (const el of document.querySelectorAll('.widget')) assert(retained.get(el.dataset.widgetId)===el, context+': WidgetCard replaced');
      await capture(context);
    }
  }
  for (let tick=0;tick<3;tick++) {
    window.probeTime += 37000;
    await pause(1100);
    check(true);
    await capture(`metrics-${tick}`);
  }
  writeSection('overlays',{version:1,items:[{...overlay,locked:true,chrome:{...overlay.chrome,background:null,border:'none',shadow:false}}]});
  await pause(500);
  check(false);
  await capture('bare-locked');
  // Remove widgets with the same overlay id: old keys must actually unmount.
  writeSection('overlays',{version:1,items:[{...overlay,locked:true,widgets:overlay.widgets.slice(0,1)}]});
  await pause(500);
  check(false);
  for (const [id,node] of retained) assert(node.isConnected === (id===overlay.widgets[0].id), 'Retained removed React subtree');
  await capture('removed-widgets-locked');
  // Strong oracle: no overlay DOM means every pixel of BOTH surfaces is alpha=0.
  writeSection('overlays',{version:1,items:[]});
  await pause(500);
  assert(!document.querySelector('.overlay,.widget,button'), 'DOM_DUPLICATE after unmount');
  await capture('unmounted',true);
  pulseProbe({kind:'done', cases, calls:window.probeCalls});
})().catch(error => pulseProbe({kind:'error', message:String(error), stack:error.stack}));
"""

bootstrap += 'window.probeInjectDuplicate = ' + json.dumps(args.inject_duplicate) + ';'

manager = WebKit2.UserContentManager()
manager.register_script_message_handler("pulseProbe")
manager.add_script(WebKit2.UserScript.new(
    bootstrap, WebKit2.UserContentInjectedFrames.TOP_FRAME,
    WebKit2.UserScriptInjectionTime.START, None, None,
))
view = WebKit2.WebView(
    user_content_manager=manager, web_context=WebKit2.WebContext.new_ephemeral(),
)
# Match Tao/Wry's transparent window: RGBA visual, app-paintable, transparent
# WebView background and SOURCE clear before the default GTK draw handler.
window = Gtk.Window(title="PULSE isolated WebKitGTK regression test")
window.set_visual(window.get_screen().get_rgba_visual())
window.set_app_paintable(True)
view.set_background_color(Gdk.RGBA(0, 0, 0, 0))


def clear_native(widget, cr):
    rect = widget.get_child().get_allocation()
    cr.rectangle(rect.x, rect.y, rect.width, rect.height)
    cr.set_source_rgba(0, 0, 0, 0)
    cr.set_operator(cairo.OPERATOR_SOURCE)
    cr.fill()
    cr.set_operator(cairo.OPERATOR_OVER)
    return False


window.connect('draw', clear_native)
window.set_decorated(False)
window.set_accept_focus(False)
window.set_default_size(360, 180)
window.add(view)
result_code = 1
captures = []
failures = []
report = {
    'gtk': [Gtk.get_major_version(), Gtk.get_minor_version(), Gtk.get_micro_version()],
    'webkit': [WebKit2.get_major_version(), WebKit2.get_minor_version(), WebKit2.get_micro_version()],
    'display': type(Gdk.Display.get_default()).__name__,
    'environment': {key: os.getenv(key) for key in [
        'WEBKIT_DISABLE_DMABUF_RENDERER', 'WEBKIT_DMABUF_RENDERER_FORCE_SHM',
        'LIBGL_ALWAYS_SOFTWARE', 'WEBKIT_DISABLE_COMPOSITING_MODE']},
    'captures': captures, 'failures': failures,
}
print(json.dumps({key: value for key, value in report.items() if key not in ('captures', 'failures')}), flush=True)


def capture(data, attempt=0):
    # Read the actual GDK surface FIRST. get_snapshot paints afresh in the web
    # process and alone cannot detect stale UI-process backing-store pixels.
    pixbuf = Gdk.pixbuf_get_from_window(window.get_window(), 0, 0, *window.get_size())
    if pixbuf is None:
        raise RuntimeError('Native surface capture unavailable; cannot claim a pixel pass')
    stem = args.output / (data['name'].replace('/', '-') + (f'-retry{attempt}' if attempt else ''))
    native_path = str(stem) + '-native.png'
    snapshot_path = str(stem) + '-webkit.png'
    pixbuf.savev(native_path, 'png', [], [])

    def snapshot_done(widget, result, _):
        try:
            surface = widget.get_snapshot_finish(result)
            surface.write_to_png(snapshot_path)
            actual = Image.open(native_path).convert('RGBA')
            expected = Image.open(snapshot_path).convert('RGBA')
            if actual.size != expected.size:
                raise RuntimeError(f'Native/WebKit size mismatch {actual.size} vs {expected.size}')
            diff = ImageChops.difference(actual, expected)
            diff.save(str(stem) + '-diff.png')
            # GPU and snapshot glyph/path edge rasterization differs. The
            # stale-paint oracle checks every locally flat reference pixel:
            # transparent holes, vacated text/control positions and chrome.
            # No global mismatch percentage can hide a faint old control.
            low = expected.filter(ImageFilter.MinFilter(3))
            high = expected.filter(ImageFilter.MaxFilter(3))
            edge = list(ImageChops.difference(low, high).getdata())
            bad = 0
            skipped = 0
            edge_pixels = 0
            scale = actual.width / data['viewport'][0]
            for index, (a, b) in enumerate(zip(actual.getdata(), expected.getdata())):
                x, y = (index % actual.width) / scale, (index // actual.width) / scale
                if not data['empty'] and any(r['x'] <= x < r['x']+r['width'] and r['y'] <= y < r['y']+r['height'] for r in data['corners']):
                    skipped += 1
                    continue
                if not data['empty'] and max(edge[index]) > 3:
                    edge_pixels += 1
                    continue
                # Compare premultiplied colors: RGB of alpha=0 is irrelevant;
                # GPU/CPU color conversion permits 6/255 color rounding.
                # Alpha is checked more strictly (2/255); full unmount below
                # permits no residual alpha at all.
                color_delta = max(abs(a[i]*a[3]/255-b[i]*b[3]/255) for i in range(3))
                if abs(a[3]-b[3]) > 2 or color_delta > 6:
                    bad += 1
            if data['empty']:
                if expected.getchannel('A').getbbox():
                    raise RuntimeError('WebKit snapshot not empty after unmount')
                bad = sum(a[3] != 0 for a in actual.getdata())
            data.update(transparentPixels=sum(a[3] == 0 for a in actual.getdata()), badPixels=bad, excludedCornerPixels=skipped, excludedEdgePixels=edge_pixels, native=native_path, webkit=snapshot_path)
            data.setdefault('attempts', []).append({'native':native_path,'webkit':snapshot_path,'badPixels':bad})
            if bad and attempt < 3:
                # WebKit renders asynchronously during native resize. Observe
                # up to 3 further natural presentations; never queue a repaint,
                # mutate CSS or clear the surface to make an assertion pass.
                GLib.timeout_add(200, lambda: capture(data, attempt+1) or False)
                return
            captures.append(data)
            if bad:
                failures.append({'kind':'STALE_NATIVE_PAINT','name':data['name'],'pixels':bad})
            print(json.dumps({'state':data['name'],'cards':len(data['cards']),'editControls':len(data['controls']),'stalePixels':bad}),flush=True)
            widget.evaluate_javascript('probeCaptured()', -1, None, None, None, None, None)
        except Exception as error:
            finish({'kind':'error','message':str(error)})

    view.get_snapshot(WebKit2.SnapshotRegion.VISIBLE, WebKit2.SnapshotOptions.TRANSPARENT_BACKGROUND, None, snapshot_done, None)


def finish(data):
    global result_code
    if data['kind'] == 'done':
        result_code = 1 if failures else 0
    else:
        failures.append(data)
    report['result'] = data
    report['passed'] = result_code == 0
    (args.output / 'report.json').write_text(json.dumps(report, indent=2))
    print(json.dumps({'result':data,'pixelStates':len(captures),'failures':len(failures),'report':str(args.output / 'report.json')}),flush=True)
    window.destroy()


def message(_manager, result):
    try:
        data = json.loads(result.get_js_value().to_string())
        kind = data['kind']
        if kind == 'minimum':
            size = data['Logical']
            geometry = Gdk.Geometry()
            geometry.min_width = round(size['width'])
            geometry.min_height = round(size['height'])
            window.set_geometry_hints(None, geometry, Gdk.WindowHints.MIN_SIZE)
        elif kind == 'resize':
            window.resize(data['width'], data['height'])
        elif kind == 'capture':
            # Same widget-level input policy as overlay_native::linux::apply.
            # WAYLAND_DEBUG=client can verify empty wl_surface input regions.
            window.input_shape_combine_region(None if data['editing'] else cairo.Region())
            capture(data)
        else:
            finish(data)
    except Exception as error:
        finish({'kind':'error','message':str(error)})


def loaded(_view, event):
    if event == WebKit2.LoadEvent.FINISHED and (_view.get_uri() or "").startswith(BASE):
        view.evaluate_javascript(RUNNER, -1, None, None, None, None, None)


def timeout():
    finish({'kind':'error','message':'WebKitGTK test timed out'})
    return False


manager.connect("script-message-received::pulseProbe", message)
view.connect("load-changed", loaded)
window.connect("destroy", lambda *_: Gtk.main_quit())
view.connect('load-failed', lambda _view, _event, _uri, error: finish({'kind':'error','message':str(error)}) or True)
try:
    window.show_all()
    view.load_uri(BASE + "/?window=overlay&id=o-probe")
    GLib.timeout_add_seconds(600, timeout)
    Gtk.main()
finally:
    stop_server()
sys.exit(result_code)
