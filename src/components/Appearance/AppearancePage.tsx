import { useMemo, useState } from 'react';
import type { ReactNode } from 'react';
import { ConfirmDialog } from '@/components/ConfirmDialog/ConfirmDialog';
import { copyText } from '@/utils/clipboard';
import type { Customization } from '@/design/appearance';
import {
  FONT_CHOICES,
  MOTION_LEVELS,
  applyUserStyle,
  chooseStyle,
  customize,
  effectiveStyle,
  enterMode,
  deleteUserStyle,
  duplicateUserStyle,
  exportUserStyle,
  importUserStyle,
  renameUserStyle,
  resetCustomization,
  saveUserStyle,
} from '@/design/appearance';
import { findMode } from '@/modes/modes';
import type { StyleId } from '@/design/styles';
import { ACCENTS, DENSITIES, PALETTES, STYLES } from '@/design/styles';
import { resolveLook } from '@/design/look';
import { useAppLook, withTransition } from '@/design/hooks';
import { updateAppearance, useAppearance } from '@/design/store';
import { CURVE_STYLES } from '@/visualization/config';
import { Choice, ColorField, Slider, Toggle } from '@/visualization/CustomizePanel';
import { Icon } from '@/components/Icon';
import { StyleCard } from '@/components/Appearance/StyleCard';
import { LookPreview } from '@/components/Appearance/LookPreview';

const FONT_LABELS: Record<(typeof FONT_CHOICES)[number], string> = {
  style: 'Style',
  sans: 'Sans',
  rounded: 'Rounded',
  condensed: 'Condensed',
  mono: 'Mono',
};

type Dialog =
  | { kind: 'save' }
  | { kind: 'rename'; id: string }
  | { kind: 'delete'; id: string }
  | { kind: 'export'; text: string }
  | { kind: 'import' };

/**
 * Appearance: choose one of PULSE's styles, tune it, save the result as your
 * own style — with a live preview drawn by the real widget engine.
 *
 * Every control either follows the style (the default) or holds the user's
 * own value; *Style* puts it back.
 */
export function AppearancePage() {
  const appearance = useAppearance();
  const look = useAppLook();
  const custom = appearance.custom;
  const [dialog, setDialog] = useState<Dialog | null>(null);
  const [name, setName] = useState('');
  const [importText, setImportText] = useState('');
  const [importError, setImportError] = useState<string | null>(null);
  const looks = useMemo(
    () => STYLES.map((style) => ({ style, look: resolveLook(style.id, custom) })),
    [custom],
  );
  const tune = (patch: Partial<Customization>) =>
    updateAppearance((section) => customize(section, patch));
  const choose = (styleId: StyleId) =>
    withTransition(() => updateAppearance((section) => chooseStyle(section, styleId)), look);
  const worn = effectiveStyle(appearance);
  const mode = findMode(appearance.activeMode);
  const t = look.style.tokens;

  return (
    <section className="page page--wide appearance">
      <header className="page-header">
        <p className="page-header__eyebrow">Studio</p>
        <h1 className="page__title">Appearance</h1>
        <p className="page__subtitle">
          Eight styles that change surfaces, depth, type, density and charts — not just colours.
          Tune any of them; save the result as your own.
        </p>
      </header>

      {mode && (
        <p className="notice" role="status">
          <strong>{mode.name} mode</strong> is on: the style you pick applies to it.{' '}
          <button
            type="button"
            className="button button--quiet"
            onClick={() =>
              withTransition(() => updateAppearance((section) => enterMode(section, null)), look)
            }
          >
            Leave {mode.name} mode
          </button>
        </p>
      )}

      <div className="style-gallery" role="group" aria-label="Styles">
        {looks.map(({ style, look: styleLook }) => (
          <StyleCard
            key={style.id}
            look={styleLook}
            name={style.name}
            tagline={style.tagline}
            active={appearance.userStyleId === null && worn === style.id}
            onSelect={() => choose(style.id)}
          />
        ))}
      </div>

      <div className="appearance__body">
        <div className="appearance__controls">
          <section className="card tune" aria-label="Your styles">
            <div className="tune__head">
              <h2 className="card__title">Your styles</h2>
              <div className="tune__actions">
                <button
                  type="button"
                  className="button button--primary"
                  onClick={() => {
                    setName(`My ${look.style.name}`);
                    setDialog({ kind: 'save' });
                  }}
                >
                  Save current look
                </button>
                <button
                  type="button"
                  className="button button--quiet"
                  onClick={() => {
                    setImportText('');
                    setImportError(null);
                    setDialog({ kind: 'import' });
                  }}
                >
                  Import
                </button>
              </div>
            </div>
            {appearance.userStyles.length === 0 ? (
              <p className="card__muted">
                Tune a style below, then save it here. Saved styles can be duplicated, renamed,
                exported and shared.
              </p>
            ) : (
              <ul className="user-styles">
                {appearance.userStyles.map((style) => {
                  const active = appearance.userStyleId === style.id;
                  const styleLook = resolveLook(style.base, style.custom);
                  return (
                    <li
                      key={style.id}
                      className={`user-style${active ? ' user-style--active' : ''}`}
                    >
                      <span className="user-style__swatches" aria-hidden="true">
                        {styleLook.viz.slice(0, 4).map((color) => (
                          <span key={color} style={{ background: color }} />
                        ))}
                      </span>
                      <span className="user-style__name">
                        {style.name}
                        <small>based on {resolveLook(style.base).style.name}</small>
                      </span>
                      <button
                        type="button"
                        className={`button${active ? ' button--active' : ''}`}
                        aria-pressed={active}
                        onClick={() =>
                          withTransition(
                            () => updateAppearance((section) => applyUserStyle(section, style.id)),
                            look,
                          )
                        }
                      >
                        {active ? 'In use' : 'Use'}
                      </button>
                      <button
                        type="button"
                        className="button button--quiet"
                        onClick={() => {
                          setName(style.name);
                          setDialog({ kind: 'rename', id: style.id });
                        }}
                      >
                        Rename
                      </button>
                      <button
                        type="button"
                        className="button button--quiet"
                        onClick={() =>
                          updateAppearance((section) => duplicateUserStyle(section, style.id))
                        }
                      >
                        Duplicate
                      </button>
                      <button
                        type="button"
                        className="button button--quiet"
                        onClick={() =>
                          setDialog({
                            kind: 'export',
                            text: JSON.stringify(exportUserStyle(style), null, 2),
                          })
                        }
                      >
                        Export
                      </button>
                      <button
                        type="button"
                        className="button button--quiet"
                        aria-label={`Delete ${style.name}`}
                        onClick={() => setDialog({ kind: 'delete', id: style.id })}
                      >
                        <Icon name="close" />
                      </button>
                    </li>
                  );
                })}
              </ul>
            )}
          </section>

          <TuneSection title="Colour">
            <div className="accent-row" role="group" aria-label="Accent colour">
              <button
                type="button"
                className={`accent-swatch accent-swatch--style${custom.accent === null ? ' accent-swatch--active' : ''}`}
                aria-pressed={custom.accent === null}
                title={`The style's accent (${t.accent})`}
                style={{ background: t.accent }}
                onClick={() => tune({ accent: null })}
              >
                <span className="sr-only">Style accent</span>
              </button>
              {ACCENTS.map((accent) => (
                <button
                  key={accent.color}
                  type="button"
                  className={`accent-swatch${custom.accent === accent.color ? ' accent-swatch--active' : ''}`}
                  aria-pressed={custom.accent === accent.color}
                  title={accent.name}
                  style={{ background: accent.color }}
                  onClick={() => tune({ accent: accent.color })}
                >
                  <span className="sr-only">{accent.name}</span>
                </button>
              ))}
            </div>
            <ColorField text="Accent" value={look.accent} onChange={(accent) => tune({ accent })} />
            <Field label="Chart palette" wide>
              <div className="palette-row" role="group" aria-label="Chart palette">
                {(['style', ...Object.keys(PALETTES)] as const).map((id) => {
                  const colors = id === 'style' ? t.viz : PALETTES[id]!.colors;
                  return (
                    <button
                      key={id}
                      type="button"
                      className={`palette-chip${custom.palette === id ? ' palette-chip--active' : ''}`}
                      aria-pressed={custom.palette === id}
                      onClick={() => tune({ palette: id })}
                    >
                      <span className="palette-chip__colors" aria-hidden="true">
                        {colors.slice(0, 5).map((color) => (
                          <span key={color} style={{ background: color }} />
                        ))}
                      </span>
                      {id === 'style' ? 'Style' : PALETTES[id]!.name}
                    </button>
                  );
                })}
              </div>
            </Field>
          </TuneSection>

          <TuneSection title="Surfaces">
            <Tuned
              label="Panel opacity"
              value={custom.surfaceOpacity}
              styleValue={t.surface.alpha}
              min={0.2}
              max={1}
              step={0.02}
              format={(v) => `${Math.round(v * 100)} %`}
              onChange={(surfaceOpacity) => tune({ surfaceOpacity })}
            />
            <Tuned
              label="Glass blur"
              value={custom.blur}
              styleValue={t.blur}
              min={0}
              max={40}
              step={1}
              format={(v) => `${v} px`}
              onChange={(blur) => tune({ blur })}
            />
            <Tuned
              label="Border strength"
              value={custom.borderStrength}
              styleValue={1}
              min={0}
              max={2}
              step={0.05}
              format={(v) => `${Math.round(v * 100)} %`}
              onChange={(borderStrength) => tune({ borderStrength })}
            />
            <Tuned
              label="Shadow depth"
              value={custom.shadowStrength}
              styleValue={1}
              min={0}
              max={2}
              step={0.05}
              format={(v) => `${Math.round(v * 100)} %`}
              onChange={(shadowStrength) => tune({ shadowStrength })}
            />
          </TuneSection>

          <TuneSection title="Shape & type">
            <Field label="Corner roundness">
              <Slider
                ariaLabel="Corner roundness"
                value={custom.radiusScale}
                min={0}
                max={2}
                step={0.05}
                format={(v) => `${Math.round(v * 100)} %`}
                onChange={(radiusScale) => tune({ radiusScale })}
              />
            </Field>
            <Field label="Font">
              <Choice
                ariaLabel="Font"
                options={FONT_CHOICES}
                value={custom.font}
                labelFor={(value) => FONT_LABELS[value]}
                onChange={(font) => tune({ font })}
              />
            </Field>
            <Field label="Text size">
              <Slider
                ariaLabel="Text size"
                value={custom.fontScale}
                min={0.85}
                max={1.3}
                step={0.01}
                format={(v) => `${Math.round(v * 100)} %`}
                onChange={(fontScale) => tune({ fontScale })}
              />
            </Field>
          </TuneSection>

          <TuneSection title="Layout & density">
            <Field label="Density">
              <Choice
                ariaLabel="Density"
                options={['style', ...DENSITIES] as const}
                value={custom.density ?? 'style'}
                labelFor={(value) =>
                  value === 'style'
                    ? `Style (${t.density})`
                    : value[0]!.toUpperCase() + value.slice(1)
                }
                onChange={(density) => tune({ density: density === 'style' ? null : density })}
              />
            </Field>
            <Tuned
              label="Widget padding"
              value={custom.widgetPadding}
              styleValue={look.widget.padding}
              min={0}
              max={24}
              step={1}
              format={(v) => `${v} px`}
              onChange={(widgetPadding) => tune({ widgetPadding })}
            />
            <Tuned
              label="Gap between widgets"
              value={custom.gap}
              styleValue={look.gridGap}
              min={2}
              max={28}
              step={1}
              format={(v) => `${v} px`}
              onChange={(gap) => tune({ gap })}
            />
            <Tuned
              label="Overlay padding"
              value={custom.overlayPadding}
              styleValue={look.style.overlay.padding}
              min={0}
              max={24}
              step={1}
              format={(v) => `${v} px`}
              onChange={(overlayPadding) => tune({ overlayPadding })}
              note="Used when an overlay takes a style."
            />
            <Tuned
              label="Overlay gap"
              value={custom.overlayGap}
              styleValue={look.style.overlay.gap}
              min={0}
              max={24}
              step={1}
              format={(v) => `${v} px`}
              onChange={(overlayGap) => tune({ overlayGap })}
            />
          </TuneSection>

          <TuneSection title="Charts">
            <Tuned
              label="Line thickness"
              value={custom.lineWidth}
              styleValue={look.style.chart.line?.width ?? 2}
              min={0.75}
              max={5}
              step={0.25}
              format={(v) => `${v} px`}
              onChange={(lineWidth) => tune({ lineWidth })}
            />
            <Tuned
              label="Fill opacity"
              value={custom.fillOpacity}
              styleValue={look.style.chart.fill?.opacity ?? 0.24}
              min={0}
              max={0.8}
              step={0.02}
              format={(v) => `${Math.round(v * 100)} %`}
              onChange={(fillOpacity) => tune({ fillOpacity })}
            />
            <Field label="Line shape">
              <Choice
                ariaLabel="Line shape"
                options={['style', ...CURVE_STYLES] as const}
                value={custom.curve ?? 'style'}
                labelFor={(value) =>
                  value === 'style' ? 'Style' : value[0]!.toUpperCase() + value.slice(1)
                }
                onChange={(curve) => tune({ curve: curve === 'style' ? null : curve })}
              />
            </Field>
            <p className="card__note">
              Applies to every chart still on its preset. Charts you customised keep their own
              settings.
            </p>
          </TuneSection>

          <TuneSection title="Content & motion">
            <div className="customize__toggles">
              <Toggle
                text="Widget titles"
                checked={custom.showTitles}
                onChange={(showTitles) => tune({ showTitles })}
              />
              <Toggle
                text="Labels in tiny widgets"
                checked={custom.microLabels}
                onChange={(microLabels) => tune({ microLabels })}
              />
            </div>
            <Field label="Motion">
              <Choice
                ariaLabel="Motion"
                options={MOTION_LEVELS}
                value={custom.motion}
                labelFor={(value) => ({ full: 'Full', reduced: 'Reduced', none: 'None' })[value]}
                onChange={(motion) => tune({ motion })}
              />
            </Field>
          </TuneSection>

          <div className="appearance__reset">
            <button
              type="button"
              className="button button--quiet"
              onClick={() => updateAppearance(resetCustomization)}
            >
              <Icon name="refresh" /> Reset every tweak to {look.style.name}
            </button>
          </div>
        </div>

        <aside className="appearance__preview" aria-label="Live preview">
          <div className="appearance__preview-sticky">
            <p className="appearance__preview-label">
              <Icon name="sparkles" /> Live preview · {look.style.name}
            </p>
            <LookPreview look={look} />
            <p className="card__note">{look.style.description}</p>
          </div>
        </aside>
      </div>

      {(dialog?.kind === 'save' || dialog?.kind === 'rename') && (
        <ConfirmDialog
          title={dialog.kind === 'save' ? 'Save this look' : 'Rename style'}
          body={[]}
          confirmLabel={dialog.kind === 'save' ? 'Save' : 'Rename'}
          tone="neutral"
          confirmDisabled={!name.trim()}
          onCancel={() => setDialog(null)}
          onConfirm={() => {
            updateAppearance((section) =>
              dialog.kind === 'save'
                ? saveUserStyle(section, name)
                : renameUserStyle(section, dialog.id, name),
            );
            setDialog(null);
          }}
        >
          <input
            type="text"
            className="customize__text dialog__input"
            aria-label="Style name"
            maxLength={40}
            value={name}
            onChange={(event) => setName(event.target.value)}
          />
        </ConfirmDialog>
      )}
      {dialog?.kind === 'delete' && (
        <ConfirmDialog
          title="Delete this style?"
          body={['Whatever wears it now keeps its look until you choose another.']}
          confirmLabel="Delete style"
          tone="danger"
          onCancel={() => setDialog(null)}
          onConfirm={() => {
            updateAppearance((section) => deleteUserStyle(section, dialog.id));
            setDialog(null);
          }}
        />
      )}
      {dialog?.kind === 'export' && (
        <ConfirmDialog
          title="Export style"
          body={['A portable JSON description of the style: colours, type and tuning only.']}
          confirmLabel="Copy to clipboard"
          tone="neutral"
          onCancel={() => setDialog(null)}
          onConfirm={() => {
            void copyText(dialog.text);
            setDialog(null);
          }}
        >
          <textarea
            className="dialog__textarea mono"
            readOnly
            value={dialog.text}
            aria-label="Exported style"
          />
        </ConfirmDialog>
      )}
      {dialog?.kind === 'import' && (
        <ConfirmDialog
          title="Import style"
          body={['Paste a PULSE style export. It is added to your styles.']}
          confirmLabel="Import"
          tone="neutral"
          confirmDisabled={!importText.trim()}
          onCancel={() => setDialog(null)}
          onConfirm={() => {
            let parsed: unknown;
            try {
              parsed = JSON.parse(importText);
            } catch {
              setImportError('This is not valid JSON.');
              return;
            }
            let error: string | undefined;
            updateAppearance((section) => {
              const result = importUserStyle(section, parsed);
              error = result.error;
              return result.section;
            });
            if (error) setImportError(error);
            else setDialog(null);
          }}
        >
          <textarea
            className="dialog__textarea mono"
            aria-label="Style JSON"
            value={importText}
            onChange={(event) => setImportText(event.target.value)}
          />
          {importError && (
            <p className="customize__error" role="alert">
              {importError}
            </p>
          )}
        </ConfirmDialog>
      )}
    </section>
  );
}

function TuneSection({
  title,
  children,
}: {
  readonly title: string;
  readonly children: ReactNode;
}) {
  return (
    <section className="card tune" aria-label={title}>
      <h2 className="card__title">{title}</h2>
      <div className="tune__body">{children}</div>
    </section>
  );
}

function Field({
  label,
  children,
  wide = false,
}: {
  readonly label: string;
  readonly children: ReactNode;
  readonly wide?: boolean;
}) {
  return (
    <div className={`tune__field${wide ? ' tune__field--wide' : ''}`}>
      <span className="tune__label">{label}</span>
      {children}
    </div>
  );
}

/**
 * A slider that follows the style until moved: it shows the style's value
 * with a *Style* mark, and *Style* puts it back.
 */
function Tuned({
  label,
  value,
  styleValue,
  min,
  max,
  step,
  format,
  onChange,
  note,
}: {
  readonly label: string;
  readonly value: number | null;
  readonly styleValue: number;
  readonly min: number;
  readonly max: number;
  readonly step: number;
  readonly format: (value: number) => string;
  readonly onChange: (value: number | null) => void;
  readonly note?: string;
}) {
  const following = value === null;
  return (
    <div className="tune__field">
      <span className="tune__label">
        {label}
        {following ? (
          <span className="tune__badge">Style</span>
        ) : (
          <button type="button" className="tune__reset" onClick={() => onChange(null)}>
            Use style
          </button>
        )}
      </span>
      <Slider
        ariaLabel={label}
        value={value ?? Math.min(max, Math.max(min, styleValue))}
        min={min}
        max={max}
        step={step}
        format={format}
        onChange={onChange}
      />
      {note && <span className="tune__note">{note}</span>}
    </div>
  );
}
