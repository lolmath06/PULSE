import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import * as metricsService from '@/services/metrics';
import * as tauri from '@/services/tauri';
import { resetMetricCatalogForTesting } from '@/hooks/useMetricCatalog';
import {
  readSection,
  resetUiConfigForTesting,
  flushUiConfig,
  initUiConfig,
  memoryBackend,
} from '@/config/uiConfig';
import { reloadVisualizationStoreForTesting } from '@/visualization/store';
import { setLiveBackendForTesting } from '@/live/liveFeed';
import { setSourceRefsForTesting, sourceRefsFrom } from '@/dashboard/bindings';
import {
  EMPTY_OVERLAYS,
  MAX_OVERLAYS,
  OVERLAY_PRESETS,
  addOverlayWidget,
  contentSize,
  createOverlay,
  createOverlayFromPreset,
  deleteOverlay,
  moveOverlayWidget,
  normalizeOverlays,
  removeOverlayWidget,
  updateOverlay,
} from '@/overlay/model';
import { normalizeSettings } from '@/overlay/settings';
import { OverlayApp } from '@/components/Overlay/OverlayApp';
import { OverlaysPage } from '@/components/Overlay/OverlaysPage';
import { SendToOverlayDialog } from '@/components/Overlay/SendToOverlayDialog';
import { MiniApp } from '@/components/Mini/MiniApp';
import { App } from '@/app/App';
import { CATALOG, SOURCE_REFS, widgetFrom } from '@/test/dashboard';

const WAYLAND_STATUS = {
  capabilities: {
    displayServer: 'wayland',
    alwaysOnTop: { status: 'limited', reason: 'Wayland has no protocol…' },
    clickThrough: { status: 'limited', reason: 'input region requested' },
    positioning: { status: 'unsupported', reason: 'Wayland clients cannot choose their position' },
    transparentWindow: { status: 'supported', reason: 'alpha' },
    globalHotkey: { status: 'limited', reason: 'XWayland only' },
    multiMonitorPositioning: { status: 'unsupported', reason: 'compositor decides' },
    tray: { status: 'limited', reason: 'GNOME needs AppIndicator' },
  },
  hotkey: 'Ctrl+Shift+F12',
  hotkeyError: null,
};

let invoke: ReturnType<typeof vi.fn>;

beforeEach(() => {
  resetUiConfigForTesting();
  reloadVisualizationStoreForTesting();
  resetMetricCatalogForTesting();
  setSourceRefsForTesting(sourceRefsFrom(SOURCE_REFS));
  setLiveBackendForTesting({
    setSubscription: () => Promise.resolve({ refused: [] }),
    buffer: () => Promise.resolve([]),
    onTick: () => () => undefined,
  });
  vi.spyOn(metricsService, 'getMetricCatalog').mockResolvedValue(CATALOG);
  invoke = vi.fn((command: string, args?: Record<string, unknown>) => {
    if (command === 'get_desktop_status') return Promise.resolve(WAYLAND_STATUS);
    if (command === 'set_overlay_hotkey') {
      return args?.shortcut === 'Ctrl+Alt+T'
        ? Promise.reject(new Error('Ctrl+Alt+T: already registered by another application'))
        : Promise.resolve(args?.shortcut ?? null);
    }
    return Promise.resolve(undefined);
  });
  vi.spyOn(tauri, 'invokeCommand').mockImplementation(invoke as never);
});

afterEach(() => {
  vi.restoreAllMocks();
  setLiveBackendForTesting(null);
  setSourceRefsForTesting(null);
});

const overlays = () => normalizeOverlays(readSection('overlays'));

describe('overlay model', () => {
  it('create, add, reorder, remove, delete', () => {
    const created = createOverlay(EMPTY_OVERLAYS, 'CPU', [widgetFrom('cpu-value')]);
    let section = created.section;
    const overlayId = created.id!;
    expect(section.items[0]!.visible).toBe(true);
    expect(section.items[0]!.locked).toBe(false);
    section = addOverlayWidget(section, overlayId, widgetFrom('memory-value'));
    const [first, second] = section.items[0]!.widgets;
    section = moveOverlayWidget(section, overlayId, second!.id, -1);
    expect(section.items[0]!.widgets.map((w) => w.id)).toEqual([second!.id, first!.id]);
    section = removeOverlayWidget(section, overlayId, first!.id);
    expect(section.items[0]!.widgets).toHaveLength(1);
    expect(deleteOverlay(section, overlayId).items).toEqual([]);
  });

  it('a widget sent to an overlay keeps metric, renderer and style, with a new id', () => {
    const widget = widgetFrom('cpu-sparkline');
    const { section } = createOverlay(EMPTY_OVERLAYS, 'x', [widget]);
    const copy = section.items[0]!.widgets[0]!;
    expect(copy.id).not.toBe(widget.id);
    expect(copy.bindings).toEqual(widget.bindings);
    expect(copy.visual).toEqual(widget.visual);
  });

  it('the window is sized to its widgets and bounded', () => {
    const { section } = createOverlay(EMPTY_OVERLAYS, 'x', [
      widgetFrom('cpu-value'),
      widgetFrom('memory-value'),
    ]);
    const overlay = section.items[0]!;
    const size = contentSize(overlay);
    expect(overlay.geometry.width).toBe(size.width);
    expect(size.width).toBe(110 + 110 + 6 + 12);
    expect(contentSize({ ...overlay, layout: 'vertical' }).height).toBe(44 + 44 + 6 + 12);
  });

  it('at most sixteen overlays', () => {
    let section = EMPTY_OVERLAYS;
    for (let i = 0; i < MAX_OVERLAYS + 3; i += 1) section = createOverlay(section, `o${i}`).section;
    expect(section.items).toHaveLength(MAX_OVERLAYS);
  });

  it('presets create editable overlays and never invent FPS', () => {
    for (const preset of OVERLAY_PRESETS) {
      const overlay = createOverlayFromPreset(EMPTY_OVERLAYS, preset).section.items[0]!;
      expect(overlay.widgets.length).toBeGreaterThan(0);
      const keys = overlay.widgets.flatMap((w) => w.bindings.map((b) => b.key));
      expect(keys.join(' ')).not.toMatch(/fps|frame/i);
    }
    const corner = createOverlayFromPreset(
      EMPTY_OVERLAYS,
      OVERLAY_PRESETS.find((p) => p.id === 'minimal-corner')!,
    );
    expect(corner.section.items[0]!.chrome.background).toBeNull();
  });

  it('stored overlays are validated: ids, geometry, colours, duplicates', () => {
    const section = normalizeOverlays({
      version: 1,
      items: [
        {
          id: 'o-good',
          name: 'A',
          geometry: { x: Number.NaN, width: -3, height: 1e9 },
          chrome: { background: 'red', opacity: 5 },
        },
        { id: 'o-good', name: 'duplicate' },
        { id: '../../evil', name: 'bad id' },
        'garbage',
      ],
    });
    expect(section.items).toHaveLength(1);
    const overlay = section.items[0]!;
    expect(overlay.geometry).toMatchObject({ x: 40, width: 24, height: 3000 });
    expect(overlay.chrome.background).toBe('#0d1013');
    expect(overlay.chrome.opacity).toBe(1);
    expect(normalizeOverlays({ version: 2, items: [] })).toEqual(EMPTY_OVERLAYS);
  });

  it('settings default to quit and Ctrl+Shift+F12', () => {
    expect(normalizeSettings(undefined)).toEqual({
      version: 1,
      closeBehavior: 'quit',
      overlayHotkey: 'Ctrl+Shift+F12',
    });
    expect(
      normalizeSettings({ closeBehavior: 'keep-running', overlayHotkey: null }).overlayHotkey,
    ).toBeNull();
  });
});

describe('overlay window', () => {
  const seed = (locked: boolean) => {
    const { section, id } = createOverlay(EMPTY_OVERLAYS, 'Game HUD', [
      widgetFrom('cpu-value'),
      widgetFrom('memory-value'),
    ]);
    resetUiConfigForTesting({ overlays: updateOverlay(section, id!, (o) => ({ ...o, locked })) });
    return id!;
  };

  it('edit mode has a drag bar, Lock, Open PULSE and a resize grip', () => {
    const id = seed(false);
    const { container } = render(<OverlayApp id={id} />);
    expect(container.querySelector('[data-tauri-drag-region]')).not.toBeNull();
    expect(screen.getByRole('button', { name: 'Lock' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Open PULSE' })).toBeInTheDocument();
    expect(container.querySelector('.overlay__resize')).not.toBeNull();
    expect(document.documentElement).toHaveClass('pulse-overlay');
  });

  it('locking removes every control; the widgets stay', async () => {
    const user = userEvent.setup();
    const id = seed(false);
    const { container } = render(<OverlayApp id={id} />);
    await user.click(screen.getByRole('button', { name: 'Lock' }));
    expect(overlays().items[0]!.locked).toBe(true);
    expect(container.querySelector('.overlay__bar')).toBeNull();
    expect(container.querySelector('.overlay__resize')).toBeNull();
    expect(screen.queryAllByRole('button')).toHaveLength(0);
    expect(container.querySelectorAll('.widget')).toHaveLength(2);
  });

  it('a change made elsewhere (the main window) appears in the overlay', () => {
    const id = seed(true);
    const { container } = render(<OverlayApp id={id} />);
    act(() => {
      resetUiConfigForTesting({
        overlays: updateOverlay(overlays(), id, (o) => ({ ...o, widgets: o.widgets.slice(0, 1) })),
      });
    });
    expect(container.querySelectorAll('.widget')).toHaveLength(1);
  });

  it('an unknown overlay id renders nothing', () => {
    const { container } = render(<OverlayApp id="o-missing" />);
    expect(container.innerHTML).toBe('');
  });

  it('the app picks the overlay or Mini surface from the URL, with a validated id', () => {
    const id = seed(true);
    window.history.pushState({}, '', `/index.html?window=overlay&id=${id}`);
    const first = render(<App />);
    expect(first.container.querySelector('.overlay')).not.toBeNull();
    first.unmount();
    window.history.pushState({}, '', '/index.html?window=overlay&id=../../evil');
    const second = render(<App />);
    expect(second.container.querySelector('.overlay')).toBeNull();
    second.unmount();
    window.history.pushState({}, '', '/index.html?window=mini');
    const third = render(<App />);
    expect(third.container.querySelector('.mini')).not.toBeNull();
    third.unmount();
    window.history.pushState({}, '', '/');
  });
});

describe('overlays page', () => {
  it('shows the honest capabilities of this session', async () => {
    render(<OverlaysPage />);
    const list = await screen.findByRole('list', { name: 'Overlay capabilities' });
    expect(within(list).getByText('Absolute positioning').parentElement!.textContent).toMatch(
      /Unsupported/,
    );
    expect(within(list).getByText('Always on top').parentElement!.textContent).toMatch(/Limited/);
    expect(screen.getByText('Wayland (native)')).toBeInTheDocument();
    await userEvent.setup().click(screen.getByRole('button', { name: /Minimal corner/ }));
    expect(screen.getByText(/drag the overlay by its bar/)).toBeInTheDocument();
    expect(screen.queryByLabelText('Overlay x')).toBeNull();
  });

  it('creates an overlay from a preset, then locks and hides it', async () => {
    const user = userEvent.setup();
    render(<OverlaysPage />);
    await user.click(screen.getByRole('button', { name: /Tiny stats/ }));
    expect(overlays().items).toHaveLength(1);
    const editor = screen.getByLabelText('Overlay Tiny stats');
    await user.click(within(editor).getByRole('button', { name: 'Lock overlay' }));
    expect(overlays().items[0]!.locked).toBe(true);
    await user.click(within(editor).getByLabelText('Visible'));
    expect(overlays().items[0]!.visible).toBe(false);
  });

  it('global actions go to the backend, which owns the windows', async () => {
    const user = userEvent.setup();
    render(<OverlaysPage />);
    for (const [label, action] of [
      ['Edit all', 'editAll'],
      ['Lock all', 'lockAll'],
      ['Show all', 'showAll'],
      ['Hide all', 'hideAll'],
    ] as const) {
      await user.click(screen.getByRole('button', { name: label }));
      expect(invoke).toHaveBeenCalledWith('overlay_action', { action });
    }
  });

  it('a conflicting shortcut is reported and not claimed', async () => {
    const user = userEvent.setup();
    render(<OverlaysPage />);
    const input = screen.getByLabelText('Global shortcut');
    await user.clear(input);
    await user.type(input, 'Ctrl+Alt+T');
    await user.click(screen.getByRole('button', { name: 'Apply' }));
    expect(await screen.findByRole('alert')).toHaveTextContent(
      /already registered.*previous shortcut is still active/,
    );

    await user.clear(input);
    await user.type(input, 'Alt+Shift+O');
    await user.click(screen.getByRole('button', { name: 'Apply' }));
    await waitFor(() => expect(screen.queryByRole('alert')).toBeNull());
    expect(invoke).toHaveBeenCalledWith('set_overlay_hotkey', { shortcut: 'Alt+Shift+O' });
  });

  it('close behaviour is a setting, quit by default', async () => {
    const user = userEvent.setup();
    render(<OverlaysPage />);
    expect(screen.getByRole('button', { name: 'Quit PULSE' })).toHaveAttribute(
      'aria-pressed',
      'true',
    );
    await user.click(
      screen.getByRole('button', { name: 'Keep running while overlays are visible' }),
    );
    expect(normalizeSettings(readSection('settings')).closeBehavior).toBe('keep-running');
  });

  it('overlays persist across a relaunch', async () => {
    const backend = memoryBackend();
    await act(() => initUiConfig(backend));
    const user = userEvent.setup();
    const first = render(<OverlaysPage />);
    await user.click(screen.getByRole('button', { name: /Gaming/ }));
    const saved = overlays();
    await act(() => flushUiConfig());
    first.unmount();
    resetUiConfigForTesting();
    await act(() => initUiConfig(backend));
    expect(overlays()).toEqual(saved);
  });
});

describe('send to overlay and Mini', () => {
  it('sends a dashboard widget to a new or an existing overlay', async () => {
    const user = userEvent.setup();
    const widget = widgetFrom('cpu-total');
    const first = render(<SendToOverlayDialog widget={widget} onClose={() => undefined} />);
    await user.click(screen.getByRole('button', { name: 'New overlay' }));
    expect(overlays().items).toHaveLength(1);
    first.unmount();
    render(<SendToOverlayDialog widget={widgetFrom('memory-value')} onClose={() => undefined} />);
    await user.click(screen.getByRole('button', { name: /Add to/ }));
    expect(overlays().items[0]!.widgets).toHaveLength(2);
  });

  it('Mini is an ordinary interactive window showing a dashboard', () => {
    render(<MiniApp />);
    expect(screen.getByLabelText('Dashboard shown in Mini')).toBeInTheDocument();
    expect(document.querySelectorAll('.widget').length).toBeGreaterThan(0);
    fireEvent.click(screen.getByRole('button', { name: 'Open PULSE' }));
    expect(invoke).toHaveBeenCalledWith('open_main_window');
  });
});
