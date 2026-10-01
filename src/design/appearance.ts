import type { CurveStyle } from '@/visualization/config';
import { CURVE_STYLES } from '@/visualization/config';
import { isHex } from '@/design/color';
import type { Density, PaletteId, StyleId } from '@/design/styles';
import { DENSITIES, PALETTES, isStyleId } from '@/design/styles';
import { isValidId, newId } from '@/dashboard/ids';
import type { ModeId } from '@/modes/modes';
import { MODE_IDS, findMode, isModeId } from '@/modes/modes';

/**
 * The `appearance` section: which style PULSE wears and how the user tuned it.
 *
 * Every customization is optional — `null` means "what the style says" — so
 * switching style keeps the user's accent or density only where they chose
 * one. Everything read back goes through {@link normalizeAppearance}: unknown
 * fields are dropped, invalid ones fall back alone, numbers are bounded.
 */

export const APPEARANCE_VERSION = 1;
export const MAX_USER_STYLES = 24;

export const FONT_CHOICES = ['style', 'sans', 'rounded', 'condensed', 'mono'] as const;
export type FontChoice = (typeof FONT_CHOICES)[number];

export const MOTION_LEVELS = ['full', 'reduced', 'none'] as const;
export type Motion = (typeof MOTION_LEVELS)[number];

export interface Customization {
  /** Replaces the style's accent (and first series colour). */
  readonly accent: string | null;
  readonly palette: PaletteId;
  /** Opacity of panels and cards, 0.2–1. */
  readonly surfaceOpacity: number | null;
  /** Backdrop blur behind translucent panels, 0–40 px. */
  readonly blur: number | null;
  /** Multiplies border visibility, 0–2. */
  readonly borderStrength: number | null;
  /** Multiplies shadow depth, 0–2. */
  readonly shadowStrength: number | null;
  /** Multiplies every font size, 0.85–1.3. */
  readonly fontScale: number;
  readonly font: FontChoice;
  /** Multiplies every corner radius, 0–2. */
  readonly radiusScale: number;
  readonly density: Density | null;
  /** Chart stroke width, 0.75–5 px. */
  readonly lineWidth: number | null;
  /** Chart fill opacity, 0–0.8. */
  readonly fillOpacity: number | null;
  readonly curve: CurveStyle | null;
  /** Inner padding of dashboard widgets, 0–24 px. */
  readonly widgetPadding: number | null;
  /** Gap between dashboard widgets, 2–28 px. */
  readonly gap: number | null;
  /** Padding and gap of overlays that follow their style, 0–24 px. */
  readonly overlayPadding: number | null;
  readonly overlayGap: number | null;
  /** Widget titles on dashboards and in Mini. */
  readonly showTitles: boolean;
  /** Labels (`CPU`) next to numbers in tiny widgets. */
  readonly microLabels: boolean;
  readonly motion: Motion;
}

export const DEFAULT_CUSTOMIZATION: Customization = {
  accent: null,
  palette: 'style',
  surfaceOpacity: null,
  blur: null,
  borderStrength: null,
  shadowStrength: null,
  fontScale: 1,
  font: 'style',
  radiusScale: 1,
  density: null,
  lineWidth: null,
  fillOpacity: null,
  curve: null,
  widgetPadding: null,
  gap: null,
  overlayPadding: null,
  overlayGap: null,
  showTitles: true,
  microLabels: true,
  motion: 'full',
};

/** A look the user saved: a built-in style plus their customization. */
export interface UserStyle {
  readonly id: string;
  readonly name: string;
  readonly base: StyleId;
  readonly custom: Customization;
}

/** What the user chose for one mode; `null` fields follow the mode. */
export interface ModeSettings {
  readonly styleId: StyleId | null;
  /** The mode's own dashboard, once made. */
  readonly dashboardId: string | null;
  readonly lockOverlays: boolean;
  readonly keepRunning: boolean;
}

export const MINI_LAYOUT_IDS = ['vitals', 'thermals', 'network', 'focus'] as const;
export type MiniLayoutId = (typeof MINI_LAYOUT_IDS)[number];

export type MiniSource =
  | { readonly kind: 'layout'; readonly id: MiniLayoutId }
  | { readonly kind: 'dashboard'; readonly id: string };

export interface MiniSettings {
  readonly source: MiniSource;
  /** `null`: Mini wears the Mini mode's style. */
  readonly styleId: StyleId | null;
}

export const DEFAULT_MINI: MiniSettings = {
  source: { kind: 'layout', id: 'vitals' },
  styleId: null,
};

export interface AppearanceSection {
  readonly version: typeof APPEARANCE_VERSION;
  readonly styleId: StyleId;
  /** A saved user style in force, or `null` for the built-in `styleId`. */
  readonly userStyleId: string | null;
  readonly custom: Customization;
  readonly userStyles: readonly UserStyle[];
  /** The first-run welcome was completed or dismissed. */
  readonly setupDone: boolean;
  /** The main window reopens where it was left. */
  readonly lastRoute: string | null;
  /** The mode PULSE is in: it then wears that mode's style. */
  readonly activeMode: ModeId | null;
  readonly modes: Readonly<Record<ModeId, ModeSettings>>;
  readonly mini: MiniSettings;
}

function defaultModeSettings(id: ModeId): ModeSettings {
  const mode = findMode(id)!;
  return {
    styleId: null,
    dashboardId: null,
    lockOverlays: mode.defaults.lockOverlays,
    keepRunning: mode.defaults.keepRunning,
  };
}

const DEFAULT_MODES = Object.fromEntries(
  MODE_IDS.map((id) => [id, defaultModeSettings(id)]),
) as Record<ModeId, ModeSettings>;

export const DEFAULT_APPEARANCE: AppearanceSection = {
  version: APPEARANCE_VERSION,
  styleId: 'clean',
  userStyleId: null,
  custom: DEFAULT_CUSTOMIZATION,
  userStyles: [],
  setupDone: false,
  lastRoute: null,
  activeMode: null,
  modes: DEFAULT_MODES,
  mini: DEFAULT_MINI,
};

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function bounded(value: unknown, min: number, max: number): number | null {
  return typeof value === 'number' && Number.isFinite(value)
    ? Math.min(max, Math.max(min, value))
    : null;
}

function oneOf<T extends string>(options: readonly T[], value: unknown): T | null {
  return typeof value === 'string' && (options as readonly string[]).includes(value)
    ? (value as T)
    : null;
}

export function normalizeCustomization(raw: unknown): Customization {
  const c = isRecord(raw) ? raw : {};
  const d = DEFAULT_CUSTOMIZATION;
  return {
    accent: isHex(c.accent) ? c.accent.toLowerCase() : null,
    palette:
      c.palette === 'style' || (typeof c.palette === 'string' && c.palette in PALETTES)
        ? (c.palette as PaletteId)
        : 'style',
    surfaceOpacity: bounded(c.surfaceOpacity, 0.2, 1),
    blur: bounded(c.blur, 0, 40),
    borderStrength: bounded(c.borderStrength, 0, 2),
    shadowStrength: bounded(c.shadowStrength, 0, 2),
    fontScale: bounded(c.fontScale, 0.85, 1.3) ?? d.fontScale,
    font: oneOf(FONT_CHOICES, c.font) ?? d.font,
    radiusScale: bounded(c.radiusScale, 0, 2) ?? d.radiusScale,
    density: oneOf(DENSITIES, c.density),
    lineWidth: bounded(c.lineWidth, 0.75, 5),
    fillOpacity: bounded(c.fillOpacity, 0, 0.8),
    curve: oneOf(CURVE_STYLES, c.curve),
    widgetPadding: bounded(c.widgetPadding, 0, 24),
    gap: bounded(c.gap, 2, 28),
    overlayPadding: bounded(c.overlayPadding, 0, 24),
    overlayGap: bounded(c.overlayGap, 0, 24),
    showTitles: typeof c.showTitles === 'boolean' ? c.showTitles : d.showTitles,
    microLabels: typeof c.microLabels === 'boolean' ? c.microLabels : d.microLabels,
    motion: oneOf(MOTION_LEVELS, c.motion) ?? d.motion,
  };
}

const ROUTE = /^\/[a-z0-9/-]{0,40}$/;

export function normalizeAppearance(raw: unknown): AppearanceSection {
  if (!isRecord(raw) || raw.version !== APPEARANCE_VERSION) return DEFAULT_APPEARANCE;
  const seen = new Set<string>();
  const userStyles: UserStyle[] = [];
  for (const entry of Array.isArray(raw.userStyles)
    ? raw.userStyles.slice(0, MAX_USER_STYLES)
    : []) {
    if (!isRecord(entry) || typeof entry.name !== 'string' || !entry.name.trim()) continue;
    const id = isValidId(entry.id) && !seen.has(entry.id) ? entry.id : newId('s');
    seen.add(id);
    userStyles.push({
      id,
      name: entry.name.trim().slice(0, 40),
      base: isStyleId(entry.base) ? entry.base : 'clean',
      custom: normalizeCustomization(entry.custom),
    });
  }
  const userStyleId =
    typeof raw.userStyleId === 'string' && userStyles.some((style) => style.id === raw.userStyleId)
      ? raw.userStyleId
      : null;
  return {
    version: APPEARANCE_VERSION,
    styleId: isStyleId(raw.styleId) ? raw.styleId : DEFAULT_APPEARANCE.styleId,
    userStyleId,
    custom: normalizeCustomization(raw.custom),
    userStyles,
    setupDone: raw.setupDone === true,
    lastRoute:
      typeof raw.lastRoute === 'string' && ROUTE.test(raw.lastRoute) ? raw.lastRoute : null,
    activeMode: isModeId(raw.activeMode) ? raw.activeMode : null,
    modes: normalizeModes(raw.modes),
    mini: normalizeMini(raw.mini),
  };
}

function normalizeModes(raw: unknown): Record<ModeId, ModeSettings> {
  const source = isRecord(raw) ? raw : {};
  return Object.fromEntries(
    MODE_IDS.map((id) => {
      const entry = isRecord(source[id]) ? (source[id] as Record<string, unknown>) : {};
      const fallback = defaultModeSettings(id);
      return [
        id,
        {
          styleId: isStyleId(entry.styleId) ? entry.styleId : null,
          dashboardId: isValidId(entry.dashboardId) ? entry.dashboardId : null,
          lockOverlays:
            typeof entry.lockOverlays === 'boolean' ? entry.lockOverlays : fallback.lockOverlays,
          keepRunning:
            typeof entry.keepRunning === 'boolean' ? entry.keepRunning : fallback.keepRunning,
        },
      ];
    }),
  ) as Record<ModeId, ModeSettings>;
}

function normalizeMini(raw: unknown): MiniSettings {
  const source = isRecord(raw) ? raw : {};
  const from = isRecord(source.source) ? source.source : {};
  const kind =
    from.kind === 'dashboard' && isValidId(from.id)
      ? ({ kind: 'dashboard', id: from.id } as const)
      : from.kind === 'layout' && (MINI_LAYOUT_IDS as readonly string[]).includes(from.id as string)
        ? ({ kind: 'layout', id: from.id as MiniLayoutId } as const)
        : DEFAULT_MINI.source;
  return { source: kind, styleId: isStyleId(source.styleId) ? source.styleId : null };
}

// --- actions -----------------------------------------------------------------

/** Wears a built-in style. The user's customization stays. */
export function setStyle(section: AppearanceSection, styleId: StyleId): AppearanceSection {
  return { ...section, styleId, userStyleId: null };
}

/** Wears a saved user style: its base and its customization. */
export function applyUserStyle(section: AppearanceSection, id: string): AppearanceSection {
  const style = section.userStyles.find((entry) => entry.id === id);
  if (!style) return section;
  return { ...section, styleId: style.base, userStyleId: id, custom: style.custom };
}

export function customize(
  section: AppearanceSection,
  patch: Partial<Customization>,
): AppearanceSection {
  return {
    ...section,
    // Tuning a saved style detaches it: the saved one stays as it was.
    userStyleId: null,
    custom: normalizeCustomization({ ...section.custom, ...patch }),
  };
}

export function resetCustomization(section: AppearanceSection): AppearanceSection {
  return { ...section, userStyleId: null, custom: DEFAULT_CUSTOMIZATION };
}

export function saveUserStyle(section: AppearanceSection, name: string): AppearanceSection {
  const trimmed = name.trim().slice(0, 40);
  if (!trimmed || section.userStyles.length >= MAX_USER_STYLES) return section;
  const style: UserStyle = {
    id: newId('s'),
    name: trimmed,
    base: section.styleId,
    custom: section.custom,
  };
  return { ...section, userStyles: [...section.userStyles, style], userStyleId: style.id };
}

export function duplicateUserStyle(section: AppearanceSection, id: string): AppearanceSection {
  const source = section.userStyles.find((entry) => entry.id === id);
  if (!source || section.userStyles.length >= MAX_USER_STYLES) return section;
  const copy = { ...source, id: newId('s'), name: `${source.name} copy`.slice(0, 40) };
  return { ...section, userStyles: [...section.userStyles, copy] };
}

export function deleteUserStyle(section: AppearanceSection, id: string): AppearanceSection {
  return {
    ...section,
    userStyles: section.userStyles.filter((entry) => entry.id !== id),
    userStyleId: section.userStyleId === id ? null : section.userStyleId,
  };
}

export function renameUserStyle(
  section: AppearanceSection,
  id: string,
  name: string,
): AppearanceSection {
  const trimmed = name.trim().slice(0, 40);
  if (!trimmed) return section;
  return {
    ...section,
    userStyles: section.userStyles.map((entry) =>
      entry.id === id ? { ...entry, name: trimmed } : entry,
    ),
  };
}

export const STYLE_EXPORT_FORMAT = 'pulse.style';

export function exportUserStyle(style: UserStyle): Record<string, unknown> {
  return { format: STYLE_EXPORT_FORMAT, version: APPEARANCE_VERSION, style };
}

export function importUserStyle(
  section: AppearanceSection,
  raw: unknown,
): { section: AppearanceSection; error?: string } {
  if (!isRecord(raw) || raw.format !== STYLE_EXPORT_FORMAT) {
    return { section, error: 'This is not a PULSE style export.' };
  }
  if (raw.version !== APPEARANCE_VERSION) {
    return { section, error: `Unsupported style export version: ${String(raw.version)}.` };
  }
  const style = isRecord(raw.style) ? raw.style : {};
  if (typeof style.name !== 'string' || !style.name.trim()) {
    return { section, error: 'The export has no style name.' };
  }
  if (section.userStyles.length >= MAX_USER_STYLES) {
    return { section, error: `At most ${MAX_USER_STYLES} saved styles.` };
  }
  const imported: UserStyle = {
    id: newId('s'),
    name: style.name.trim().slice(0, 40),
    base: isStyleId(style.base) ? style.base : 'clean',
    custom: normalizeCustomization(style.custom),
  };
  return { section: { ...section, userStyles: [...section.userStyles, imported] } };
}

export function setLastRoute(section: AppearanceSection, route: string): AppearanceSection {
  if (!ROUTE.test(route) || section.lastRoute === route) return section;
  return { ...section, lastRoute: route };
}

export function completeSetup(section: AppearanceSection): AppearanceSection {
  return section.setupDone ? section : { ...section, setupDone: true };
}

// --- modes ---------------------------------------------------------------------

/** The style PULSE wears: the active mode's, or the chosen one. */
export function effectiveStyle(section: AppearanceSection): StyleId {
  if (!section.activeMode) return section.styleId;
  return section.modes[section.activeMode].styleId ?? findMode(section.activeMode)!.style;
}

/** The style of a mode: the user's choice, or the mode's own. */
export function modeStyle(section: AppearanceSection, id: ModeId): StyleId {
  return section.modes[id].styleId ?? findMode(id)!.style;
}

export function enterMode(section: AppearanceSection, id: ModeId | null): AppearanceSection {
  return section.activeMode === id ? section : { ...section, activeMode: id };
}

export function updateMode(
  section: AppearanceSection,
  id: ModeId,
  patch: Partial<ModeSettings>,
): AppearanceSection {
  return { ...section, modes: { ...section.modes, [id]: { ...section.modes[id], ...patch } } };
}

/**
 * Chooses a style from the Studio: it goes to the active mode when there is
 * one (that is what is on screen), otherwise it becomes the app's style.
 */
export function chooseStyle(section: AppearanceSection, styleId: StyleId): AppearanceSection {
  if (section.activeMode) {
    return { ...updateMode(section, section.activeMode, { styleId }), userStyleId: null };
  }
  return setStyle(section, styleId);
}

export function setMini(
  section: AppearanceSection,
  patch: Partial<MiniSettings>,
): AppearanceSection {
  return { ...section, mini: { ...section.mini, ...patch } };
}
