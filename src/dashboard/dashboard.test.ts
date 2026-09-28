import { describe, expect, it } from 'vitest';
import type { WidgetInstance } from '@/dashboard/model';
import { KIND_LIMITS, clampRect, normalizeWidget } from '@/dashboard/model';
import {
  compact,
  firstFit,
  moveWidget,
  overlaps,
  reflow,
  repairLayout,
  resizeWidget,
  toPixels,
} from '@/dashboard/layout';
import {
  activeDashboard,
  addWidget,
  createDashboard,
  defaultSection,
  deleteDashboard,
  duplicateDashboard,
  duplicateWidget,
  exportDashboard,
  importDashboard,
  normalizeDashboards,
  removeWidget,
  renameDashboard,
  resetDashboard,
  setLocked,
  updateWidget,
  visualDefaultsFor,
} from '@/dashboard/dashboards';
import {
  deleteTemplate,
  instantiateTemplate,
  normalizeTemplates,
  renameTemplate,
  saveTemplate,
  EMPTY_TEMPLATES,
} from '@/dashboard/templates';
import { BLUEPRINTS, blueprintAvailability, findBlueprint } from '@/dashboard/library';
import { autoPick, persistableRef, resolveBinding, sourceRefsFrom } from '@/dashboard/bindings';
import { CATALOG, SOURCE_REFS, widgetFrom } from '@/test/dashboard';

const at = (
  widget: WidgetInstance,
  x: number,
  y: number,
  w: number,
  h: number,
): WidgetInstance => ({
  ...widget,
  layout: { x, y, w, h },
});

function noOverlaps(widgets: readonly WidgetInstance[]) {
  for (const a of widgets) {
    for (const b of widgets) {
      if (a !== b) expect(overlaps(a.layout, b.layout), `${a.id} vs ${b.id}`).toBe(false);
    }
  }
}

describe('grid layout', () => {
  const a = at(widgetFrom('cpu-total', { id: 'a' }), 0, 0, 6, 4);
  const b = at(widgetFrom('memory', { id: 'b' }), 6, 0, 6, 4);
  const c = at(widgetFrom('network', { id: 'c' }), 0, 4, 4, 4);

  it('clamps position and size to the grid and the kind', () => {
    const value = KIND_LIMITS.value;
    expect(clampRect({ x: -5, y: -3, w: 99, h: 99 }, 'value')).toEqual({
      x: 0,
      y: 0,
      w: value.maxW,
      h: value.maxH,
    });
    expect(clampRect({ x: 11, y: 0, w: 4, h: 2 }, 'value').x).toBe(8);
    expect(
      clampRect({ x: Number.NaN, y: Number.POSITIVE_INFINITY, w: -1, h: 0 }, 'visualization'),
    ).toEqual({
      x: 0,
      y: 0,
      w: KIND_LIMITS.visualization.minW,
      h: KIND_LIMITS.visualization.minH,
    });
  });

  it('moving onto a widget pushes it down instead of overlapping', () => {
    const moved = moveWidget([a, b, c], 'c', 6, 0);
    noOverlaps(moved);
    expect(moved.find((w) => w.id === 'c')!.layout).toMatchObject({ x: 6, y: 0 });
    expect(moved.find((w) => w.id === 'b')!.layout.y).toBeGreaterThanOrEqual(4);
  });

  it('resizing respects the kind limits and repairs overlaps', () => {
    const resized = resizeWidget([a, b, c], 'a', 12, 20);
    const widget = resized.find((w) => w.id === 'a')!;
    expect(widget.layout.w).toBe(12);
    expect(widget.layout.h).toBe(KIND_LIMITS.visualization.maxH);
    noOverlaps(resized);
  });

  it('compaction floats widgets up without reordering them', () => {
    const floating = compact([at(a, 0, 5, 6, 4), at(b, 6, 9, 6, 4)]);
    expect(floating.map((w) => w.layout.y)).toEqual([0, 0]);
  });

  it('a corrupted stored layout is repaired: overlaps removed, bounds enforced', () => {
    const repaired = repairLayout([at(a, 0, 0, 6, 4), at(b, 2, 1, 6, 4), at(c, 11, 0, 4, 4)]);
    noOverlaps(repaired);
    for (const widget of repaired)
      expect(widget.layout.x + widget.layout.w).toBeLessThanOrEqual(12);
    expect(repairLayout(repaired)).toEqual(repaired);
  });

  it('finds the first free spot for a new widget', () => {
    expect(firstFit([a, b], 4, 4)).toEqual({ x: 0, y: 4 });
    expect(firstFit([], 4, 4)).toEqual({ x: 0, y: 0 });
  });

  it('reflows for narrow screens without touching the stored layout', () => {
    const stored = [a, b, c];
    const six = reflow(stored, 6);
    noOverlaps(six);
    for (const widget of six) expect(widget.layout.x + widget.layout.w).toBeLessThanOrEqual(6);
    const one = reflow(stored, 1);
    expect(one.map((w) => w.layout.x)).toEqual([0, 0, 0]);
    expect(new Set(one.map((w) => w.layout.y)).size).toBe(3);
    expect(stored[0]!.layout).toEqual({ x: 0, y: 0, w: 6, h: 4 });
  });

  it('converts cells to pixels', () => {
    expect(toPixels({ x: 1, y: 2, w: 3, h: 2 }, 70, 44, 10)).toEqual({
      left: 80,
      top: 108,
      width: 230,
      height: 98,
    });
  });
});

describe('widget normalisation', () => {
  it('rejects garbage and repairs every invalid field', () => {
    expect(normalizeWidget(null)).toBeNull();
    expect(normalizeWidget({ bindings: [] })).toBeNull();
    const widget = normalizeWidget(
      {
        id: 'Robert"); DROP',
        kind: 'hologram',
        bindings: [
          { key: 'cpu.usage.total', source: { mode: 'fixed', ref: 'network:mac-00 11' } },
          { key: 'NOT A KEY' },
        ],
        visual: { config: { renderer: 'pie3d', colors: { primary: 'red' } }, range: '2y' },
        frame: { padding: -4, opacity: 7, background: 'blue' },
        layout: { x: Number.NaN, y: -1, w: 1e9, h: -3 },
        size: { width: 1e9, height: Number.NaN },
      },
      visualDefaultsFor,
    )!;
    expect(widget.id).toMatch(/^[a-z0-9-]+$/);
    expect(widget.kind).toBe('visualization');
    expect(widget.bindings).toEqual([
      { key: 'cpu.usage.total', source: { mode: 'auto' }, label: null },
    ]);
    expect(widget.visual.config.renderer).toBe('area');
    expect(widget.visual.config.colors.primary).toMatch(/^#[0-9a-f]{6}$/);
    expect(widget.visual.range).toBe('15m');
    expect(widget.frame.padding).toBe(0);
    expect(widget.frame.opacity).toBe(1);
    expect(widget.frame.background).toBeNull();
    expect(widget.layout).toEqual({ x: 0, y: 0, w: 12, h: 2 });
    expect(widget.size.width).toBe(1600);
    expect(Number.isFinite(widget.size.height)).toBe(true);
  });

  it('a stored widget round-trips exactly', () => {
    const widget = widgetFrom('cpu-group');
    expect(normalizeWidget(JSON.parse(JSON.stringify(widget)), visualDefaultsFor)).toEqual(widget);
  });
});

describe('dashboards', () => {
  it('an absent or foreign section is the default dashboard, with stable ids', () => {
    for (const raw of [undefined, null, 5, { version: 9 }, { version: 1, items: [] }]) {
      const section = normalizeDashboards(raw);
      expect(section.items).toHaveLength(1);
      expect(section.items[0]!.name).toBe('Default');
    }
    expect(normalizeDashboards(undefined)).toEqual(normalizeDashboards(undefined));
  });

  it('the default dashboard starts with CPU Total, memory, thermal, GPU, network and storage', () => {
    const keys = defaultSection().items[0]!.widgets.flatMap((w) => w.bindings.map((b) => b.key));
    for (const key of [
      'cpu.usage.total',
      'memory.usage.percent',
      'cpu.temperature.package',
      'gpu.usage.core',
      'network.receive.bytes_per_second',
      'storage.io.read.bytes_per_second',
    ]) {
      expect(keys).toContain(key);
    }
    expect(defaultSection().items[0]!.widgets.length).toBeLessThanOrEqual(8);
  });

  it('duplicates, dedupes and falls back to a valid active dashboard', () => {
    const one = defaultSection().items[0]!;
    const section = normalizeDashboards({
      version: 1,
      activeId: 'nope',
      items: [one, one, { name: 'Broken', widgets: 'x' }],
    });
    expect(new Set(section.items.map((item) => item.id)).size).toBe(section.items.length);
    expect(section.activeId).toBe(section.items[0]!.id);
  });

  it('create, rename, duplicate, delete — the last one is protected', () => {
    let section = createDashboard(defaultSection(), 'Gaming');
    expect(activeDashboard(section).name).toBe('Gaming');
    const gaming = section.activeId;
    section = renameDashboard(section, gaming, '  Development ');
    expect(activeDashboard(section).name).toBe('Development');
    section = duplicateDashboard(section, 'default');
    const copy = activeDashboard(section);
    expect(copy.name).toBe('Default copy');
    const originalIds = section.items[0]!.widgets.map((w) => w.id);
    expect(copy.widgets.some((w) => originalIds.includes(w.id))).toBe(false);
    section = deleteDashboard(section, copy.id);
    section = deleteDashboard(section, gaming);
    expect(section.items).toHaveLength(1);
    expect(deleteDashboard(section, 'default').items).toHaveLength(1);
  });

  it('add, duplicate (new id, same metric/renderer/style/size), update and remove widgets', () => {
    let section = createDashboard(defaultSection(), 'Mine');
    const id = section.activeId;
    const cpu = widgetFrom('cpu-sparkline');
    section = addWidget(section, id, cpu);
    section = duplicateWidget(section, id, cpu.id);
    const [original, copy] = activeDashboard(section).widgets;
    expect(copy!.id).not.toBe(original!.id);
    expect(copy!.bindings).toEqual(original!.bindings);
    expect(copy!.visual).toEqual(original!.visual);
    expect(copy!.layout.w).toBe(original!.layout.w);
    expect(overlaps(copy!.layout, original!.layout)).toBe(false);

    section = updateWidget(section, id, copy!.id, (w) => ({ ...w, title: 'Tiny CPU' }));
    expect(activeDashboard(section).widgets[1]!.title).toBe('Tiny CPU');
    section = removeWidget(section, id, original!.id);
    expect(activeDashboard(section).widgets.map((w) => w.id)).toEqual([copy!.id]);
  });

  it('lock and unlock', () => {
    const unlocked = setLocked(defaultSection(), 'default', false);
    expect(activeDashboard(unlocked).locked).toBe(false);
    expect(activeDashboard(setLocked(unlocked, 'default', true)).locked).toBe(true);
  });

  it('reset restores the defaults and does not touch templates', () => {
    let section = createDashboard(defaultSection(), 'X');
    section = addWidget(section, section.activeId, widgetFrom('wifi'));
    const templates = saveTemplate(EMPTY_TEMPLATES, 'Keep me', widgetFrom('cpu-value'));
    section = resetDashboard(section, section.activeId);
    expect(activeDashboard(section).widgets.length).toBe(defaultSection().items[0]!.widgets.length);
    expect(templates.items).toHaveLength(1);
  });

  it('export → import gives a new dashboard with new ids and no hardware id', () => {
    const dashboard = activeDashboard(defaultSection());
    const exported = JSON.parse(JSON.stringify(exportDashboard(dashboard)));
    expect(JSON.stringify(exported)).not.toMatch(/mac-|serial-|wwid-|\/home\//);
    const { section, error } = importDashboard(defaultSection(), exported);
    expect(error).toBeUndefined();
    expect(section.items).toHaveLength(2);
    const imported = activeDashboard(section);
    expect(imported.id).not.toBe(dashboard.id);
    expect(imported.widgets.map((w) => w.bindings)).toEqual(
      dashboard.widgets.map((w) => w.bindings),
    );
    expect(importDashboard(defaultSection(), { format: 'other' }).error).toMatch(/not a PULSE/);
    expect(
      importDashboard(defaultSection(), { format: 'pulse.dashboard', version: 7 }).error,
    ).toMatch(/version/);
  });
});

describe('templates', () => {
  it('save, rename, delete, instantiate with a new id', () => {
    const widget = widgetFrom('cpu-value', { title: 'CPU tiny' });
    let section = saveTemplate(EMPTY_TEMPLATES, 'My CPU tiny', widget);
    const template = section.items[0]!;
    expect(template.name).toBe('My CPU tiny');
    const made = instantiateTemplate(template);
    expect(made.id).not.toBe(widget.id);
    expect(made.visual).toEqual(widget.visual);
    section = renameTemplate(section, template.id, 'Renamed');
    expect(section.items[0]!.name).toBe('Renamed');
    expect(normalizeTemplates(JSON.parse(JSON.stringify(section)))).toEqual(section);
    expect(deleteTemplate(section, template.id).items).toEqual([]);
  });
});

describe('library and bindings', () => {
  const refs = sourceRefsFrom(SOURCE_REFS);

  it('every blueprint uses the generic kinds, and CPU Total is cpu.usage.total', () => {
    expect(findBlueprint('cpu-total')!.bindings.map((b) => b.key)).toEqual(['cpu.usage.total']);
    for (const blueprint of BLUEPRINTS) {
      expect(['visualization', 'value', 'group', 'summary']).toContain(blueprint.kind);
    }
  });

  it('an entry the machine cannot show is disabled with the backend reason', () => {
    const gpu = blueprintAvailability(findBlueprint('gpu')!, CATALOG);
    expect(gpu).toEqual({ ok: false, reason: 'Not supported: libnvidia-ml.so.1 is not installed' });
    expect(blueprintAvailability(findBlueprint('wifi')!, CATALOG)).toMatchObject({ ok: false });
    expect(blueprintAvailability(findBlueprint('cpu-total')!, CATALOG)).toEqual({ ok: true });
    expect(blueprintAvailability(findBlueprint('thermal')!, CATALOG)).toEqual({ ok: true });
  });

  it('automatic network choice prefers hardware over virtual interfaces', () => {
    const candidates = CATALOG.filter((d) => d.metric.key === 'network.receive.bytes_per_second');
    expect(autoPick('network.receive.bytes_per_second', candidates, CATALOG)!.metric.sourceId).toBe(
      'network:mac-001122334455',
    );
  });

  it('a fixed source is stored as its persistable reference and resolved back', () => {
    const ref = persistableRef('storage:serial-bbb', refs)!;
    expect(ref).not.toContain('serial');
    const resolved = resolveBinding(
      { key: 'storage.io.read.bytes_per_second', source: { mode: 'fixed', ref }, label: null },
      CATALOG,
      refs,
    );
    expect(resolved.ok && resolved.ref.sourceId).toBe('storage:serial-bbb');
  });

  it('without the backend map, a device source is never stored raw', () => {
    const empty = sourceRefsFrom({});
    expect(persistableRef('network:mac-001122334455', empty)).toBeNull();
    expect(persistableRef('cpu:logical-3', empty)).toBe('cpu:logical-3');
  });

  it('a source that disappeared is "Source unavailable" with remap candidates', () => {
    const resolved = resolveBinding(
      {
        key: 'storage.io.read.bytes_per_second',
        source: { mode: 'fixed', ref: 'storage:ffffffffffffffff' },
        label: null,
      },
      CATALOG,
      refs,
    );
    expect(resolved.ok).toBe(false);
    if (!resolved.ok) {
      expect(resolved.reason).toBe('Source unavailable');
      expect(resolved.candidates).toHaveLength(2);
    }
  });
});
