import type { WidgetInstance } from '@/dashboard/model';
import { normalizeWidget } from '@/dashboard/model';
import { visualDefaultsFor } from '@/dashboard/dashboards';
import { isValidId, newId } from '@/dashboard/ids';
import type { Overlay } from '@/overlay/model';
import { normalizeOverlay } from '@/overlay/model';

/**
 * User widget templates — "My CPU tiny" — reusable on any dashboard and in
 * any overlay. The built-in starting points are the library's blueprints,
 * which cannot be renamed or deleted; these can.
 */

export const TEMPLATES_VERSION = 1;
export const MAX_TEMPLATES = 64;
export const MAX_USER_PACKS = 32;

export interface WidgetTemplate {
  readonly id: string;
  readonly name: string;
  readonly widget: WidgetInstance;
}

/** An overlay the user saved to reuse: its widgets, layout, style and chrome. */
export interface UserOverlayPack {
  readonly id: string;
  readonly name: string;
  readonly overlay: Overlay;
}

export interface TemplatesSection {
  readonly version: typeof TEMPLATES_VERSION;
  readonly items: readonly WidgetTemplate[];
  /** The user's own overlay packs. */
  readonly overlays: readonly UserOverlayPack[];
}

export const EMPTY_TEMPLATES: TemplatesSection = {
  version: TEMPLATES_VERSION,
  items: [],
  overlays: [],
};

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

export function normalizeTemplates(raw: unknown): TemplatesSection {
  if (!isRecord(raw) || raw.version !== TEMPLATES_VERSION || !Array.isArray(raw.items)) {
    return EMPTY_TEMPLATES;
  }
  const items: WidgetTemplate[] = [];
  for (const entry of raw.items.slice(0, MAX_TEMPLATES)) {
    if (!isRecord(entry) || typeof entry.name !== 'string' || !entry.name.trim()) continue;
    const widget = normalizeWidget(entry.widget, visualDefaultsFor);
    if (!widget) continue;
    items.push({
      id: isValidId(entry.id) ? entry.id : newId('t'),
      name: entry.name.trim().slice(0, 40),
      widget,
    });
  }
  const overlays: UserOverlayPack[] = [];
  for (const entry of Array.isArray(raw.overlays) ? raw.overlays.slice(0, MAX_USER_PACKS) : []) {
    if (!isRecord(entry) || typeof entry.name !== 'string' || !entry.name.trim()) continue;
    const overlay = normalizeOverlay(entry.overlay);
    if (!overlay) continue;
    overlays.push({
      id: isValidId(entry.id) ? entry.id : newId('p'),
      name: entry.name.trim().slice(0, 40),
      overlay,
    });
  }
  return { version: TEMPLATES_VERSION, items, overlays };
}

/** Saves an overlay as one of the user's packs (widgets, layout, style, chrome). */
export function saveUserPack(
  section: TemplatesSection,
  name: string,
  overlay: Overlay,
): TemplatesSection {
  const trimmed = name.trim().slice(0, 40);
  if (!trimmed || section.overlays.length >= MAX_USER_PACKS) return section;
  const stored: Overlay = { ...overlay, id: 'o-pack', visible: true, locked: false, origin: null };
  return {
    ...section,
    overlays: [...section.overlays, { id: newId('p'), name: trimmed, overlay: stored }],
  };
}

export function deleteUserPack(section: TemplatesSection, id: string): TemplatesSection {
  return { ...section, overlays: section.overlays.filter((pack) => pack.id !== id) };
}

export function saveTemplate(
  section: TemplatesSection,
  name: string,
  widget: WidgetInstance,
): TemplatesSection {
  const trimmed = name.trim().slice(0, 40);
  if (!trimmed || section.items.length >= MAX_TEMPLATES) return section;
  return {
    ...section,
    items: [
      ...section.items,
      { id: newId('t'), name: trimmed, widget: { ...widget, id: 'template' } },
    ],
  };
}

export function renameTemplate(
  section: TemplatesSection,
  id: string,
  name: string,
): TemplatesSection {
  const trimmed = name.trim().slice(0, 40);
  if (!trimmed) return section;
  return {
    ...section,
    items: section.items.map((item) => (item.id === id ? { ...item, name: trimmed } : item)),
  };
}

export function deleteTemplate(section: TemplatesSection, id: string): TemplatesSection {
  return { ...section, items: section.items.filter((item) => item.id !== id) };
}

/** A new widget from a template: same metric, renderer, style and size; new id. */
export function instantiateTemplate(template: WidgetTemplate): WidgetInstance {
  return { ...template.widget, id: newId('w') };
}
