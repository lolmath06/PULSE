import { beforeEach, describe, expect, it } from 'vitest';
import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { readSection, resetUiConfigForTesting } from '@/config/uiConfig';
import {
  DEFAULT_APPEARANCE,
  DEFAULT_CUSTOMIZATION,
  applyUserStyle,
  customize,
  deleteUserStyle,
  exportUserStyle,
  importUserStyle,
  normalizeAppearance,
  resetCustomization,
  saveUserStyle,
  setLastRoute,
  setStyle,
} from '@/design/appearance';
import { contrast } from '@/design/color';
import {
  frameFollowsStyle,
  overlayStyleChrome,
  resolveLook,
  styleWidget,
  tokensFor,
} from '@/design/look';
import { STYLES, STYLE_IDS } from '@/design/styles';
import { createWidget, findBlueprint } from '@/dashboard/library';
import { DEFAULT_FRAME } from '@/dashboard/model';
import { applyOverlayStyle, createOverlayFromPreset, OVERLAY_PRESETS } from '@/overlay/model';
import { EMPTY_OVERLAYS } from '@/overlay/model';
import { AppearancePage } from '@/components/Appearance/AppearancePage';
import { RootLook } from '@/design/LookContext';

const cpuChart = () => createWidget(findBlueprint('cpu-total')!);

describe('appearance section', () => {
  it('falls back to the default for anything unusable', () => {
    expect(normalizeAppearance(undefined)).toEqual(DEFAULT_APPEARANCE);
    expect(normalizeAppearance({ version: 99 })).toEqual(DEFAULT_APPEARANCE);
    const read = normalizeAppearance({
      version: 1,
      styleId: 'hologram',
      custom: { accent: 'red', fontScale: 9, blur: -3, density: 'huge', palette: 'nope' },
      lastRoute: 'javascript:alert(1)',
    });
    expect(read.styleId).toBe('clean');
    expect(read.custom.accent).toBeNull();
    expect(read.custom.fontScale).toBe(1.3);
    expect(read.custom.blur).toBe(0);
    expect(read.custom.density).toBeNull();
    expect(read.custom.palette).toBe('style');
    expect(read.lastRoute).toBeNull();
  });

  it('keeps a valid route and style', () => {
    const read = normalizeAppearance({ version: 1, styleId: 'neon', lastRoute: '/dashboard' });
    expect(read.styleId).toBe('neon');
    expect(setLastRoute(read, '/gaming').lastRoute).toBe('/gaming');
    expect(setLastRoute(read, 'http://x').lastRoute).toBe('/dashboard');
  });

  it('switching style keeps the user tuning; resetting clears it', () => {
    const tuned = customize(DEFAULT_APPEARANCE, { accent: '#ff3b5c', density: 'compact' });
    const switched = setStyle(tuned, 'glass');
    expect(switched.custom.accent).toBe('#ff3b5c');
    expect(resetCustomization(switched).custom).toEqual(DEFAULT_CUSTOMIZATION);
  });

  it('saves, applies, detaches and deletes user styles', () => {
    let section = customize(setStyle(DEFAULT_APPEARANCE, 'neon'), { accent: '#a3e635' });
    section = saveUserStyle(section, '  Night lime  ');
    const saved = section.userStyles[0]!;
    expect(saved).toMatchObject({ name: 'Night lime', base: 'neon' });
    expect(section.userStyleId).toBe(saved.id);

    section = customize(setStyle(section, 'clean'), { accent: null });
    expect(section.userStyleId).toBeNull();
    section = applyUserStyle(section, saved.id);
    expect(section.styleId).toBe('neon');
    expect(section.custom.accent).toBe('#a3e635');

    // Tuning a saved style detaches it and leaves the saved one intact.
    section = customize(section, { blur: 12 });
    expect(section.userStyleId).toBeNull();
    expect(section.userStyles[0]!.custom.blur).toBeNull();

    section = deleteUserStyle(section, saved.id);
    expect(section.userStyles).toEqual([]);
  });

  it('exports and imports styles, refusing anything else', () => {
    const section = saveUserStyle(customize(DEFAULT_APPEARANCE, { accent: '#60a5fa' }), 'Blue');
    const exported = JSON.parse(JSON.stringify(exportUserStyle(section.userStyles[0]!)));
    const { section: imported, error } = importUserStyle(DEFAULT_APPEARANCE, exported);
    expect(error).toBeUndefined();
    expect(imported.userStyles[0]).toMatchObject({ name: 'Blue', base: 'clean' });
    expect(imported.userStyles[0]!.custom.accent).toBe('#60a5fa');
    expect(importUserStyle(DEFAULT_APPEARANCE, { format: 'pulse.dashboard' }).error).toMatch(
      /not a PULSE style/,
    );
    expect(importUserStyle(DEFAULT_APPEARANCE, { ...exported, version: 7 }).error).toMatch(
      /Unsupported/,
    );
  });
});

describe('styles', () => {
  it('ships eight distinct styles', () => {
    expect(STYLE_IDS).toHaveLength(8);
    // Genuinely different, not colour swaps: shape, depth, type or density differ.
    const signatures = new Set(
      STYLES.map(({ tokens: t }) =>
        [t.radius, t.blur, t.chamfer, t.fontDisplay, t.density, t.glow > 0, t.valueHalo].join('|'),
      ),
    );
    expect(signatures.size).toBe(STYLES.length);
  });

  it('keeps text readable on every style', () => {
    for (const style of STYLES) {
      const t = style.tokens;
      expect(contrast(t.text, t.bg), `${style.id} text`).toBeGreaterThanOrEqual(7);
      expect(contrast(t.textMuted, t.bg), `${style.id} muted`).toBeGreaterThanOrEqual(4.5);
    }
  });

  it('never lets a style change what belongs to the metric', () => {
    for (const style of STYLES) {
      const chart = style.chart as Record<string, unknown>;
      expect(chart.renderer, style.id).toBeUndefined();
      expect(chart.size, style.id).toBeUndefined();
      expect(chart.scale, style.id).toBeUndefined();
      const colors = (chart.colors ?? {}) as Record<string, unknown>;
      expect(colors.thresholds, style.id).toBeUndefined();
      expect(colors.series, style.id).toBeUndefined();
    }
  });
});

describe('looks and tokens', () => {
  it('density scales spacing and the dashboard gap', () => {
    const compact = resolveLook('clean', { ...DEFAULT_CUSTOMIZATION, density: 'compact' });
    const spacious = resolveLook('clean', { ...DEFAULT_CUSTOMIZATION, density: 'spacious' });
    expect(compact.gridGap).toBeLessThan(10);
    expect(spacious.gridGap).toBeGreaterThan(10);
    expect(resolveLook('clean', { ...DEFAULT_CUSTOMIZATION, gap: 20 }).gridGap).toBe(20);
  });

  it('a chosen accent leads the chart palette', () => {
    const look = resolveLook('glass', { ...DEFAULT_CUSTOMIZATION, accent: '#ff3b5c' });
    expect(look.accent).toBe('#ff3b5c');
    expect(tokensFor(look)['--pulse-viz-1']).toBe('#ff3b5c');
    expect(tokensFor(look)['--pulse-accent']).toBe('#ff3b5c');
  });

  it('turns tuning into tokens', () => {
    const look = resolveLook('glass', {
      ...DEFAULT_CUSTOMIZATION,
      surfaceOpacity: 0.5,
      blur: 30,
      motion: 'none',
      radiusScale: 0,
    });
    const tokens = tokensFor(look);
    expect(tokens['--pulse-blur']).toBe('30px');
    expect(tokens['--pulse-surface']).toMatch(/rgba\(255, 255, 255, 0\.5\)/);
    expect(tokens['--pulse-motion']).toBe('0ms');
    expect(tokens['--pulse-radius']).toBe('0px');
    for (const name of ['--pulse-bg', '--pulse-shadow', '--pulse-font', '--pulse-space-4']) {
      expect(tokens[name], name).toBeTruthy();
    }
  });

  it('gives each style its own overlay chrome; the HUD has none', () => {
    const hud = overlayStyleChrome(resolveLook('hud'));
    expect(hud.chrome.background).toBeNull();
    expect(hud.chrome.opacity).toBe(0);
    const glass = overlayStyleChrome(resolveLook('glass'));
    expect(glass.chrome.background).not.toBeNull();
    expect(glass.chrome.border).toBe('thin');
  });

  it('an overlay taking a style is refitted to the new padding', () => {
    const { section, id } = createOverlayFromPreset(EMPTY_OVERLAYS, OVERLAY_PRESETS[0]!);
    const before = section.items[0]!;
    const look = overlayStyleChrome(
      resolveLook('glass', { ...DEFAULT_CUSTOMIZATION, overlayPadding: 20 }),
    );
    const after = applyOverlayStyle(section, id!, 'glass', look).items[0]!;
    expect(after.styleId).toBe('glass');
    expect(after.chrome.padding).toBe(20);
    expect(after.geometry.width).toBeGreaterThan(before.geometry.width);
  });
});

describe('styling widgets at render time', () => {
  it('a chart on its preset takes the style, keeping what belongs to the metric', () => {
    const widget = cpuChart();
    const styled = styleWidget(widget, resolveLook('technical'));
    const config = styled.visual.config;
    expect(config.line.curve).toBe('straight');
    expect(config.line.points).toBe('small');
    expect(config.renderer).toBe(widget.visual.config.renderer);
    expect(config.scale).toEqual(widget.visual.config.scale);
    expect(config.colors.thresholds).toEqual(widget.visual.config.colors.thresholds);
    // The stored widget is never changed.
    expect(widget.visual.config.line.curve).toBe('smooth');
  });

  it('a customised chart keeps exactly what the user chose', () => {
    const widget = cpuChart();
    const custom = {
      ...widget,
      visual: { ...widget.visual, modified: true, config: { ...widget.visual.config } },
    };
    const styled = styleWidget(custom, resolveLook('technical'));
    expect(styled.visual.config.line.curve).toBe('smooth');
  });

  it('global chart tuning, titles and micro labels follow the switches', () => {
    const look = resolveLook('clean', {
      ...DEFAULT_CUSTOMIZATION,
      lineWidth: 4,
      curve: 'stepped',
      showTitles: false,
      microLabels: false,
    });
    const chart = styleWidget(cpuChart(), look);
    expect(chart.visual.config.line.width).toBe(4);
    expect(chart.visual.config.line.curve).toBe('stepped');
    expect(chart.frame.showTitle).toBe(false);
    const value = styleWidget(createWidget(findBlueprint('cpu-value')!), look);
    expect(value.visual.config.text.showLabel).toBe(false);
  });

  it('an untouched frame takes the style shape; a tuned one keeps its own', () => {
    const widget = cpuChart();
    expect(frameFollowsStyle(widget.frame)).toBe(true);
    const glass = resolveLook('glass');
    expect(styleWidget(widget, glass).frame.radius).toBe(glass.widget.radius);
    const tuned = { ...widget, frame: { ...DEFAULT_FRAME, radius: 3 } };
    expect(styleWidget(tuned, glass).frame.radius).toBe(3);
  });
});

describe('appearance page', () => {
  beforeEach(() => resetUiConfigForTesting());

  function renderPage() {
    return render(
      <RootLook look={resolveLook('clean')}>
        <AppearancePage />
      </RootLook>,
    );
  }

  it('chooses a style, tunes it and saves it as the user’s own', async () => {
    const user = userEvent.setup();
    renderPage();
    const gallery = screen.getByRole('group', { name: 'Styles' });
    expect(within(gallery).getAllByRole('button')).toHaveLength(8);

    await user.click(within(gallery).getByRole('button', { name: 'Neon style' }));
    expect(normalizeAppearance(readSection('appearance')).styleId).toBe('neon');

    await user.click(screen.getByRole('button', { name: 'Crimson' }));
    expect(normalizeAppearance(readSection('appearance')).custom.accent).toBe('#ff3b5c');

    await user.click(screen.getByRole('button', { name: 'Compact' }));
    expect(normalizeAppearance(readSection('appearance')).custom.density).toBe('compact');

    await user.click(screen.getByRole('button', { name: 'Save current look' }));
    const input = screen.getByLabelText('Style name');
    await user.clear(input);
    await user.type(input, 'Red neon');
    await user.click(screen.getByRole('button', { name: 'Save' }));
    const saved = normalizeAppearance(readSection('appearance')).userStyles;
    expect(saved).toHaveLength(1);
    expect(saved[0]).toMatchObject({ name: 'Red neon', base: 'neon' });
    expect(screen.getByText('Red neon')).toBeInTheDocument();
  });

  it('shows a live preview drawn by the widget engine', () => {
    renderPage();
    const preview = screen.getByLabelText('Preview');
    expect(within(preview).getAllByRole('article').length).toBeGreaterThanOrEqual(5);
  });
});
