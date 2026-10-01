import { beforeEach, describe, expect, it } from 'vitest';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryRouter, RouterProvider } from 'react-router-dom';
import { readSection, resetUiConfigForTesting } from '@/config/uiConfig';
import {
  DEFAULT_APPEARANCE,
  chooseStyle,
  effectiveStyle,
  enterMode,
  normalizeAppearance,
  updateMode,
} from '@/design/appearance';
import { appLook } from '@/design/look';
import { normalizeDashboards, defaultSection } from '@/dashboard/dashboards';
import { GRID_COLUMNS } from '@/dashboard/model';
import { overlaps } from '@/dashboard/layout';
import { EMPTY_OVERLAYS, normalizeOverlays, overlayLayout } from '@/overlay/model';
import {
  FOOTPRINTS,
  OVERLAY_PACKS,
  createOverlayFromPack,
  findPack,
  resetOverlayToPack,
} from '@/presets/overlayPacks';
import {
  DASHBOARD_TEMPLATES,
  createDashboardFromTemplate,
  findTemplate,
  resetDashboardToOrigin,
  templateWidgets,
} from '@/presets/dashboardTemplates';
import { MODES, findMode } from '@/modes/modes';
import { MINI_LAYOUTS } from '@/modes/miniLayouts';
import { fitLine } from '@/visualization/presentation';
import { STYLE_IDS } from '@/design/styles';
import { MiniApp } from '@/components/Mini/MiniApp';
import { Welcome } from '@/components/Welcome/Welcome';
import { ModePage } from '@/components/Modes/ModePage';

const SCREEN = { width: 2560, height: 1440 };

const allKeys = () => [
  ...OVERLAY_PACKS.flatMap((pack) => pack.widgets().flatMap((w) => w.bindings.map((b) => b.key))),
  ...DASHBOARD_TEMPLATES.flatMap((t) => t.widgets().flatMap((w) => w.bindings.map((b) => b.key))),
  ...MINI_LAYOUTS.flatMap((l) => l.rows().flatMap((r) => r.widget.bindings.map((b) => b.key))),
];

describe('overlay packs', () => {
  it('ships twelve curated packs covering every footprint', () => {
    expect(OVERLAY_PACKS).toHaveLength(12);
    expect(new Set(OVERLAY_PACKS.map((pack) => pack.id)).size).toBe(12);
    for (const footprint of FOOTPRINTS) {
      expect(
        OVERLAY_PACKS.some((pack) => pack.footprint === footprint),
        footprint,
      ).toBe(true);
    }
    for (const pack of OVERLAY_PACKS) {
      expect(STYLE_IDS).toContain(pack.styleId);
      expect(pack.widgets().length, pack.id).toBeGreaterThan(0);
    }
  });

  it('never shows a number PULSE cannot measure: no FPS, no ping', () => {
    for (const key of allKeys()) {
      expect(key).not.toMatch(/fps|frame|ping|latency/i);
    }
  });

  it('a top bar spans the screen at the top; a bottom bar sits on the bottom edge', () => {
    const top = createOverlayFromPack(EMPTY_OVERLAYS, findPack('top-bar')!, SCREEN);
    const overlay = top.section.items[0]!;
    expect(overlay).toMatchObject({ span: 'fill', styleId: 'clean', origin: { pack: 'top-bar' } });
    expect(overlay.geometry).toMatchObject({ x: 0, y: 0, width: 2560 });
    // The widgets are spread across the whole width.
    const layout = overlayLayout(overlay);
    expect(layout.width).toBe(2560);
    const last = layout.boxes.at(-1)!;
    expect(last.x + last.width).toBeGreaterThan(2500);

    const bottom = createOverlayFromPack(EMPTY_OVERLAYS, findPack('bottom-bar')!, SCREEN);
    const g = bottom.section.items[0]!.geometry;
    expect(g.y + g.height).toBe(1440);
    expect(g.width).toBe(2560);
  });

  it('rails run the full height on their edge; the corner HUD sits in the top-right', () => {
    const right = createOverlayFromPack(EMPTY_OVERLAYS, findPack('right-rail')!, SCREEN).section
      .items[0]!;
    expect(right.geometry.height).toBe(1440);
    expect(right.geometry.x + right.geometry.width).toBe(2560);
    expect(overlayLayout(right).height).toBe(1440);
    const left = createOverlayFromPack(EMPTY_OVERLAYS, findPack('left-rail')!, SCREEN).section
      .items[0]!;
    expect(left.geometry.x).toBe(0);
    const corner = createOverlayFromPack(EMPTY_OVERLAYS, findPack('gaming-corner')!, SCREEN).section
      .items[0]!;
    expect(corner.geometry.x + corner.geometry.width).toBeLessThan(2560);
    expect(corner.geometry.y).toBeLessThan(100);
    expect(corner.layout).toBe('grid');
  });

  it('packs survive a save and reload, and reset to themselves', () => {
    const { section, id } = createOverlayFromPack(
      EMPTY_OVERLAYS,
      findPack('summary-card')!,
      SCREEN,
    );
    const reloaded = normalizeOverlays(JSON.parse(JSON.stringify(section)));
    expect(reloaded).toEqual(section);
    const edited = {
      ...section,
      items: section.items.map((item) => ({
        ...item,
        widgets: item.widgets.slice(1),
        geometry: { ...item.geometry, x: 500, y: 300 },
      })),
    };
    const reset = resetOverlayToPack(edited, id!).items[0]!;
    expect(reset.widgets).toHaveLength(section.items[0]!.widgets.length);
    expect(reset.geometry).toMatchObject({ x: 500, y: 300 });
    expect(reset.id).toBe(id);
  });
});

describe('dashboard templates', () => {
  it('ships eight composed templates that fit the grid without overlap', () => {
    expect(DASHBOARD_TEMPLATES).toHaveLength(8);
    expect(DASHBOARD_TEMPLATES.filter((t) => t.featured).map((t) => t.id)).toEqual(['showcase']);
    for (const template of DASHBOARD_TEMPLATES) {
      const widgets = templateWidgets(template);
      expect(widgets.length, template.id).toBeGreaterThan(2);
      for (const [i, a] of widgets.entries()) {
        expect(a.layout.x + a.layout.w, template.id).toBeLessThanOrEqual(GRID_COLUMNS);
        for (const b of widgets.slice(i + 1)) {
          expect(overlaps(a.layout, b.layout), `${template.id}: ${a.id} × ${b.id}`).toBe(false);
        }
      }
      // As designed: repair moves nothing.
      expect(widgets.map((w) => w.layout)).toEqual(template.widgets().map((w) => w.layout));
    }
  });

  it('a dashboard from a template wears its style, remembers its origin and resets to it', () => {
    const template = findTemplate('showcase')!;
    const { section, id } = createDashboardFromTemplate(defaultSection(), template);
    const dashboard = section.items.find((item) => item.id === id)!;
    expect(section.activeId).toBe(id);
    expect(dashboard).toMatchObject({ styleId: 'glass', origin: { template: 'showcase' } });
    expect(normalizeDashboards(JSON.parse(JSON.stringify(section)))).toEqual(section);

    const emptied = {
      ...section,
      items: section.items.map((item) =>
        item.id === id ? { ...item, widgets: [], styleId: null } : item,
      ),
    };
    const reset = resetDashboardToOrigin(emptied, id!).items.find((item) => item.id === id)!;
    expect(reset.widgets).toHaveLength(dashboard.widgets.length);
    expect(reset.styleId).toBe('glass');
  });
});

describe('modes', () => {
  it('each mode points at a real template, real packs and a style', () => {
    expect(MODES.map((mode) => mode.id)).toEqual(['gaming', 'development', 'personal', 'mini']);
    for (const mode of MODES) {
      expect(findTemplate(mode.template), mode.id).toBeDefined();
      for (const pack of mode.packs) expect(findPack(pack), `${mode.id}: ${pack}`).toBeDefined();
      expect(STYLE_IDS).toContain(mode.style);
    }
    expect(findMode('gaming')!.defaults.lockOverlays).toBe(true);
  });

  it('entering a mode makes PULSE wear its style and density; leaving restores the chosen one', () => {
    let section = enterMode(DEFAULT_APPEARANCE, 'development');
    expect(effectiveStyle(section)).toBe('technical');
    expect(appLook(section).density).toBe('compact');
    section = updateMode(section, 'development', { styleId: 'neon' });
    expect(appLook(section).style.id).toBe('neon');
    section = enterMode(section, null);
    expect(appLook(section).style.id).toBe('clean');
  });

  it('a style chosen in the Studio goes to the active mode, else to the app', () => {
    const inGaming = chooseStyle(enterMode(DEFAULT_APPEARANCE, 'gaming'), 'stealth');
    expect(inGaming.modes.gaming.styleId).toBe('stealth');
    expect(inGaming.styleId).toBe('clean');
    expect(chooseStyle(DEFAULT_APPEARANCE, 'glass').styleId).toBe('glass');
  });

  it('mode settings and Mini survive normalisation, invalid values fall back', () => {
    const read = normalizeAppearance({
      version: 1,
      activeMode: 'raid',
      modes: { gaming: { styleId: 'hud', lockOverlays: false, dashboardId: 'Bad Id!' } },
      mini: { source: { kind: 'layout', id: 'thermals' }, styleId: 'nope' },
    });
    expect(read.activeMode).toBeNull();
    expect(read.modes.gaming).toMatchObject({
      styleId: 'hud',
      lockOverlays: false,
      dashboardId: null,
    });
    expect(read.modes.development.keepRunning).toBe(false);
    expect(read.mini).toEqual({ source: { kind: 'layout', id: 'thermals' }, styleId: null });
  });
});

describe('micro widgets', () => {
  it('keep their label at overlay sizes — the value is never sacrificed', () => {
    const fit = fitLine(106, 40, { label: 'RAM', value: '30', unit: '%' }, 1, 32);
    expect(fit.label).toBe(true);
    expect(fit.fontPx).toBeGreaterThanOrEqual(9);
    const tight = fitLine(40, 16, { label: 'CPU temperature', value: '71', unit: '°C' });
    expect(tight.label).toBe(false);
  });
});

describe('Mini and the welcome', () => {
  beforeEach(() => resetUiConfigForTesting());

  it('Mini shows its own layout by default and switches to a dashboard', () => {
    render(<MiniApp />);
    const select = screen.getByLabelText('Shown in Mini');
    expect((select as HTMLSelectElement).value).toBe('layout:vitals');
    expect(document.querySelectorAll('.widget').length).toBe(4);
    fireEvent.change(select, { target: { value: 'layout:focus' } });
    expect(normalizeAppearance(readSection('appearance')).mini.source).toEqual({
      kind: 'layout',
      id: 'focus',
    });
  });

  it('the welcome sets up a mode, its style, a dashboard and a starter overlay', async () => {
    const user = userEvent.setup();
    let done = false;
    render(<Welcome onDone={() => (done = true)} />);
    await user.click(
      within(screen.getByRole('group', { name: 'Mode' })).getByRole('button', { name: /Gaming/ }),
    );
    await user.click(screen.getByRole('button', { name: /Start PULSE/ }));
    expect(done).toBe(true);
    const appearance = normalizeAppearance(readSection('appearance'));
    expect(appearance).toMatchObject({ setupDone: true, activeMode: 'gaming' });
    expect(appearance.modes.gaming.styleId).toBe('gaming');
    const dashboards = normalizeDashboards(readSection('dashboards'));
    const created = dashboards.items.find(
      (item) => item.id === appearance.modes.gaming.dashboardId,
    );
    expect(created?.origin?.template).toBe('gaming');
    await waitFor(() =>
      expect(normalizeOverlays(readSection('overlays')).items[0]?.origin?.pack).toBe(
        'gaming-corner',
      ),
    );
  });

  it('skipping the welcome only marks it done', async () => {
    render(<Welcome onDone={() => undefined} />);
    await userEvent.setup().click(screen.getByRole('button', { name: 'Skip' }));
    expect(normalizeAppearance(readSection('appearance')).setupDone).toBe(true);
    expect(readSection('overlays')).toBeUndefined();
  });

  it('a mode page creates the mode dashboard from its template and enters the mode', async () => {
    const user = userEvent.setup();
    const router = createMemoryRouter(
      [{ path: '/', element: <ModePage mode={findMode('development')!} /> }],
      { initialEntries: ['/'] },
    );
    render(<RouterProvider router={router} />);
    await user.click(screen.getByRole('button', { name: /Create Development dashboard/ }));
    const appearance = normalizeAppearance(readSection('appearance'));
    expect(appearance.modes.development.dashboardId).not.toBeNull();
    await user.click(screen.getByRole('button', { name: /Enter Development mode/ }));
    await waitFor(() =>
      expect(normalizeAppearance(readSection('appearance')).activeMode).toBe('development'),
    );
    const packs = screen.getByLabelText('Overlays for Development');
    expect(
      within(packs).getByRole('article', { name: 'Dev Monitor Rail overlay pack' }),
    ).toBeInTheDocument();
  });
});
