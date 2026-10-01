import { useMemo, useState } from 'react';
import type { CSSProperties } from 'react';
import type { DesktopStatus } from '@/overlay/desktop';
import { placementHint } from '@/overlay/bridgeGuide';
import type { Footprint, OverlayPack } from '@/presets/overlayPacks';
import {
  FOOTPRINTS,
  FOOTPRINT_LABELS,
  OVERLAY_PACKS,
  createOverlayFromPack,
} from '@/presets/overlayPacks';
import { primaryScreen } from '@/overlay/screen';
import { updateOverlays, useOverlays } from '@/overlay/store';
import { MAX_OVERLAYS } from '@/overlay/model';
import { readAppearance } from '@/design/store';
import { resolveLook } from '@/design/look';
import { StyleScope } from '@/design/LookContext';
import { styleById } from '@/design/styles';
import { Icon } from '@/components/Icon';

/** Where a footprint sits on the miniature screen, in % of it. */
const SILHOUETTE: Readonly<Record<Footprint, CSSProperties>> = {
  micro: { left: '7%', top: '12%' },
  card: { left: '7%', top: '12%' },
  'top-strip': { left: 0, top: 0, right: 0 },
  'bottom-strip': { left: 0, bottom: 0, right: 0 },
  'left-rail': { left: 0, top: 0, bottom: 0 },
  'right-rail': { right: 0, top: 0, bottom: 0 },
  corner: { right: '5%', top: '9%' },
  block: { left: '7%', top: '10%' },
};

/**
 * A pack as a card: where it sits on a screen, how it reads, in its own style.
 * Static on purpose — browsing twelve packs costs no metric subscription.
 */
export function PackCard({
  pack,
  hint,
  onAdd,
  disabled,
}: {
  readonly pack: OverlayPack;
  readonly hint: string | null;
  readonly onAdd: () => void;
  readonly disabled?: boolean;
}) {
  const look = useMemo(() => resolveLook(pack.styleId), [pack.styleId]);
  return (
    <article className="pack-card" aria-label={`${pack.name} overlay pack`}>
      <StyleScope look={look} className="pack-card__screen">
        <span className="pack-card__wallpaper" aria-hidden="true" />
        <span
          className={`pack-card__overlay pack-card__overlay--${pack.layout}${
            pack.span === 'fill' ? ' pack-card__overlay--fill' : ''
          }`}
          style={SILHOUETTE[pack.footprint]}
          aria-hidden="true"
        >
          {pack.sample.map((item) => (
            <span key={item} className="pack-card__item">
              {item}
            </span>
          ))}
        </span>
      </StyleScope>
      <div className="pack-card__body">
        <div className="pack-card__title">
          <h3>{pack.name}</h3>
          <span className="pack-card__chips">
            <span className="chip">{FOOTPRINT_LABELS[pack.footprint]}</span>
            <span className="chip chip--style">{styleById(pack.styleId).name}</span>
          </span>
        </div>
        <p className="pack-card__description">{pack.description}</p>
        {hint && pack.needs && (
          <p className="pack-card__hint" title={hint}>
            <Icon name="overlays" /> {hint}
          </p>
        )}
        <button
          type="button"
          className="button button--primary pack-card__add"
          disabled={disabled}
          onClick={onAdd}
        >
          <Icon name="plus" /> Add overlay
        </button>
      </div>
    </article>
  );
}

/**
 * The built-in overlay packs, filterable by footprint. `only` limits them to
 * a mode's suggestions, in that order.
 */
export function PackGallery({
  status,
  only,
  onAdded,
}: {
  readonly status: DesktopStatus | null;
  readonly only?: readonly string[];
  readonly onAdded?: (id: string, pack: OverlayPack) => void;
}) {
  const overlays = useOverlays();
  const [filter, setFilter] = useState<Footprint | 'all'>('all');
  const packs = only
    ? only.map((id) => OVERLAY_PACKS.find((pack) => pack.id === id)).filter((p) => p !== undefined)
    : OVERLAY_PACKS;
  const shown = filter === 'all' ? packs : packs.filter((pack) => pack.footprint === filter);
  const full = overlays.items.length >= MAX_OVERLAYS;
  const hint = placementHint(status);
  const footprints = FOOTPRINTS.filter((footprint) => packs.some((p) => p.footprint === footprint));

  const add = (pack: OverlayPack) => {
    void primaryScreen().then((screen) => {
      let created: string | null = null;
      updateOverlays((section) => {
        const result = createOverlayFromPack(section, pack, screen, readAppearance().custom);
        created = result.id;
        return result.section;
      });
      if (created) onAdded?.(created, pack);
    });
  };

  return (
    <div className="pack-gallery">
      {!only && (
        <div className="segmented pack-gallery__filter" role="group" aria-label="Footprint">
          {(['all', ...footprints] as const).map((value) => (
            <button
              key={value}
              type="button"
              className={`segmented__option${filter === value ? ' segmented__option--active' : ''}`}
              aria-pressed={filter === value}
              onClick={() => setFilter(value)}
            >
              {value === 'all' ? 'All' : FOOTPRINT_LABELS[value]}
            </button>
          ))}
        </div>
      )}
      <div className="pack-gallery__grid">
        {shown.map((pack) => (
          <PackCard key={pack.id} pack={pack} hint={hint} disabled={full} onAdd={() => add(pack)} />
        ))}
      </div>
      {full && (
        <p className="card__note">At most {MAX_OVERLAYS} overlays. Delete one to add another.</p>
      )}
    </div>
  );
}
