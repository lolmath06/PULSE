import { useMemo, useState } from 'react';
import type { ReactNode } from 'react';
import { Trans, useTranslation } from 'react-i18next';
import { LanguageSelect } from '@/components/Language/LanguageSelect';
import { formatFixed } from '@/i18n/format';
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

/** A ratio as a whole percentage in the active locale: `85 %`. */
const percent = (v: number) => `${formatFixed(v * 100, 0)} %`;

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
  const { t } = useTranslation();
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
  const tokens = look.style.tokens;

  return (
    <section className="page page--wide appearance">
      <header className="page-header">
        <p className="page-header__eyebrow">{t('nav.groups.studio')}</p>
        <h1 className="page__title">{t('nav.routes.appearance.label')}</h1>
        <p className="page__subtitle">{t('appearance.subtitle')}</p>
      </header>

      <section className="card tune appearance__language" aria-label={t('language.label')}>
        <LanguageSelect />
      </section>

      {mode && (
        <p className="notice" role="status">
          <Trans
            i18nKey="appearance.modeOn"
            values={{ mode: t(`modes.${mode.id}.name`) }}
            components={{ strong: <strong /> }}
          />{' '}
          <button
            type="button"
            className="button button--quiet"
            onClick={() =>
              withTransition(() => updateAppearance((section) => enterMode(section, null)), look)
            }
          >
            {t('appearance.leaveMode', { mode: t(`modes.${mode.id}.name`) })}
          </button>
        </p>
      )}

      <div className="style-gallery" role="group" aria-label={t('appearance.ui.styles')}>
        {looks.map(({ style, look: styleLook }) => (
          <StyleCard
            key={style.id}
            look={styleLook}
            name={t(`styles.${style.id}.name`)}
            tagline={t(`styles.${style.id}.tagline`)}
            active={appearance.userStyleId === null && worn === style.id}
            onSelect={() => choose(style.id)}
          />
        ))}
      </div>

      <div className="appearance__body">
        <div className="appearance__controls">
          <section className="card tune" aria-label={t('appearance.ui.yourStyles')}>
            <div className="tune__head">
              <h2 className="card__title">{t('appearance.ui.yourStyles')}</h2>
              <div className="tune__actions">
                <button
                  type="button"
                  className="button button--primary"
                  onClick={() => {
                    setName(t('appearance.myStyle', { name: t(`styles.${look.style.id}.name`) }));
                    setDialog({ kind: 'save' });
                  }}
                >
                  {t('appearance.saveCurrent')}
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
                  {t('common.import')}
                </button>
              </div>
            </div>
            {appearance.userStyles.length === 0 ? (
              <p className="card__muted">{t('appearance.noUserStyles')}</p>
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
                        <small>
                          {t('appearance.basedOn', {
                            name: t(`styles.${resolveLook(style.base).style.id}.name`),
                          })}
                        </small>
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
                        {active ? t('appearance.inUse') : t('appearance.use')}
                      </button>
                      <button
                        type="button"
                        className="button button--quiet"
                        onClick={() => {
                          setName(style.name);
                          setDialog({ kind: 'rename', id: style.id });
                        }}
                      >
                        {t('common.rename')}
                      </button>
                      <button
                        type="button"
                        className="button button--quiet"
                        onClick={() =>
                          updateAppearance((section) => duplicateUserStyle(section, style.id))
                        }
                      >
                        {t('common.duplicate')}
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
                        {t('common.export')}
                      </button>
                      <button
                        type="button"
                        className="button button--quiet"
                        aria-label={t('common.deleteNamed', { name: style.name })}
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

          <TuneSection title={t('appearance.ui.colour')}>
            <div className="accent-row" role="group" aria-label={t('appearance.ui.accentColour')}>
              <button
                type="button"
                className={`accent-swatch accent-swatch--style${custom.accent === null ? ' accent-swatch--active' : ''}`}
                aria-pressed={custom.accent === null}
                title={t('appearance.styleAccentTitle', { color: tokens.accent })}
                style={{ background: tokens.accent }}
                onClick={() => tune({ accent: null })}
              >
                <span className="sr-only">{t('appearance.styleAccent')}</span>
              </button>
              {ACCENTS.map((accent) => (
                <button
                  key={accent.color}
                  type="button"
                  className={`accent-swatch${custom.accent === accent.color ? ' accent-swatch--active' : ''}`}
                  aria-pressed={custom.accent === accent.color}
                  title={t(`styles.accents.${accent.id}`)}
                  style={{ background: accent.color }}
                  onClick={() => tune({ accent: accent.color })}
                >
                  <span className="sr-only">{t(`styles.accents.${accent.id}`)}</span>
                </button>
              ))}
            </div>
            <ColorField
              text={t('appearance.ui.accent')}
              value={look.accent}
              onChange={(accent) => tune({ accent })}
            />
            <Field label={t('appearance.ui.chartPalette')} wide>
              <div
                className="palette-row"
                role="group"
                aria-label={t('appearance.ui.chartPalette')}
              >
                {(['style', ...Object.keys(PALETTES)] as const).map((id) => {
                  const colors = id === 'style' ? tokens.viz : PALETTES[id]!.colors;
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
                      {id === 'style' ? t('nav.style') : t(`styles.palettes.${id}`)}
                    </button>
                  );
                })}
              </div>
            </Field>
          </TuneSection>

          <TuneSection title={t('appearance.ui.surfaces')}>
            <Tuned
              label={t('appearance.ui.panelOpacity')}
              value={custom.surfaceOpacity}
              styleValue={tokens.surface.alpha}
              min={0.2}
              max={1}
              step={0.02}
              format={percent}
              onChange={(surfaceOpacity) => tune({ surfaceOpacity })}
            />
            <Tuned
              label={t('appearance.ui.glassBlur')}
              value={custom.blur}
              styleValue={tokens.blur}
              min={0}
              max={40}
              step={1}
              format={(v) => `${v} px`}
              onChange={(blur) => tune({ blur })}
            />
            <Tuned
              label={t('appearance.ui.borderStrength')}
              value={custom.borderStrength}
              styleValue={1}
              min={0}
              max={2}
              step={0.05}
              format={percent}
              onChange={(borderStrength) => tune({ borderStrength })}
            />
            <Tuned
              label={t('appearance.ui.shadowDepth')}
              value={custom.shadowStrength}
              styleValue={1}
              min={0}
              max={2}
              step={0.05}
              format={percent}
              onChange={(shadowStrength) => tune({ shadowStrength })}
            />
          </TuneSection>

          <TuneSection title={t('appearance.ui.shapeType')}>
            <Field label={t('appearance.ui.cornerRoundness')}>
              <Slider
                ariaLabel={t('appearance.ui.cornerRoundness')}
                value={custom.radiusScale}
                min={0}
                max={2}
                step={0.05}
                format={percent}
                onChange={(radiusScale) => tune({ radiusScale })}
              />
            </Field>
            <Field label={t('appearance.ui.font')}>
              <Choice
                ariaLabel={t('appearance.ui.font')}
                options={FONT_CHOICES}
                value={custom.font}
                labelFor={(value) => t(`appearance.fonts.${value}`)}
                onChange={(font) => tune({ font })}
              />
            </Field>
            <Field label={t('appearance.ui.textSize')}>
              <Slider
                ariaLabel={t('appearance.ui.textSize')}
                value={custom.fontScale}
                min={0.85}
                max={1.3}
                step={0.01}
                format={percent}
                onChange={(fontScale) => tune({ fontScale })}
              />
            </Field>
          </TuneSection>

          <TuneSection title={t('appearance.ui.layoutDensity')}>
            <Field label={t('appearance.ui.density')}>
              <Choice
                ariaLabel={t('appearance.ui.density')}
                options={['style', ...DENSITIES] as const}
                value={custom.density ?? 'style'}
                labelFor={(value) =>
                  value === 'style'
                    ? t('appearance.styleWith', {
                        value: t(`appearance.densities.${tokens.density}`),
                      })
                    : t(`appearance.densities.${value}`)
                }
                onChange={(density) => tune({ density: density === 'style' ? null : density })}
              />
            </Field>
            <Tuned
              label={t('appearance.ui.widgetPadding')}
              value={custom.widgetPadding}
              styleValue={look.widget.padding}
              min={0}
              max={24}
              step={1}
              format={(v) => `${v} px`}
              onChange={(widgetPadding) => tune({ widgetPadding })}
            />
            <Tuned
              label={t('appearance.ui.gapBetweenWidgets')}
              value={custom.gap}
              styleValue={look.gridGap}
              min={2}
              max={28}
              step={1}
              format={(v) => `${v} px`}
              onChange={(gap) => tune({ gap })}
            />
            <Tuned
              label={t('appearance.ui.overlayPadding')}
              value={custom.overlayPadding}
              styleValue={look.style.overlay.padding}
              min={0}
              max={24}
              step={1}
              format={(v) => `${v} px`}
              onChange={(overlayPadding) => tune({ overlayPadding })}
              note={t('appearance.ui.overlayPaddingNote')}
            />
            <Tuned
              label={t('appearance.ui.overlayGap')}
              value={custom.overlayGap}
              styleValue={look.style.overlay.gap}
              min={0}
              max={24}
              step={1}
              format={(v) => `${v} px`}
              onChange={(overlayGap) => tune({ overlayGap })}
            />
          </TuneSection>

          <TuneSection title={t('appearance.ui.charts')}>
            <Tuned
              label={t('appearance.ui.lineThickness')}
              value={custom.lineWidth}
              styleValue={look.style.chart.line?.width ?? 2}
              min={0.75}
              max={5}
              step={0.25}
              format={(v) => `${v} px`}
              onChange={(lineWidth) => tune({ lineWidth })}
            />
            <Tuned
              label={t('appearance.ui.fillOpacity')}
              value={custom.fillOpacity}
              styleValue={look.style.chart.fill?.opacity ?? 0.24}
              min={0}
              max={0.8}
              step={0.02}
              format={percent}
              onChange={(fillOpacity) => tune({ fillOpacity })}
            />
            <Field label={t('appearance.ui.lineShape')}>
              <Choice
                ariaLabel={t('appearance.ui.lineShape')}
                options={['style', ...CURVE_STYLES] as const}
                value={custom.curve ?? 'style'}
                labelFor={(value) =>
                  value === 'style' ? t('nav.style') : t(`viz.options.${value}`)
                }
                onChange={(curve) => tune({ curve: curve === 'style' ? null : curve })}
              />
            </Field>
            <p className="card__note">{t('appearance.chartsNote')}</p>
          </TuneSection>

          <TuneSection title={t('appearance.ui.contentMotion')}>
            <div className="customize__toggles">
              <Toggle
                text={t('appearance.ui.widgetTitles')}
                checked={custom.showTitles}
                onChange={(showTitles) => tune({ showTitles })}
              />
              <Toggle
                text={t('appearance.ui.labelsInTinyWidgets')}
                checked={custom.microLabels}
                onChange={(microLabels) => tune({ microLabels })}
              />
            </div>
            <Field label={t('appearance.ui.motion')}>
              <Choice
                ariaLabel={t('appearance.ui.motion')}
                options={MOTION_LEVELS}
                value={custom.motion}
                labelFor={(value) => t(`appearance.motionLevels.${value}`)}
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
              <Icon name="refresh" />{' '}
              {t('appearance.resetAll', { name: t(`styles.${look.style.id}.name`) })}
            </button>
          </div>
        </div>

        <aside className="appearance__preview" aria-label={t('appearance.ui.livePreview')}>
          <div className="appearance__preview-sticky">
            <p className="appearance__preview-label">
              <Icon name="sparkles" />{' '}
              {t('appearance.livePreviewNamed', { name: t(`styles.${look.style.id}.name`) })}
            </p>
            <LookPreview look={look} />
            <p className="card__note">{t(`styles.${look.style.id}.description`)}</p>
          </div>
        </aside>
      </div>

      {(dialog?.kind === 'save' || dialog?.kind === 'rename') && (
        <ConfirmDialog
          title={dialog.kind === 'save' ? t('appearance.saveLook') : t('appearance.renameStyle')}
          body={[]}
          confirmLabel={dialog.kind === 'save' ? t('common.save') : t('common.rename')}
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
            aria-label={t('appearance.ui.styleName')}
            maxLength={40}
            value={name}
            onChange={(event) => setName(event.target.value)}
          />
        </ConfirmDialog>
      )}
      {dialog?.kind === 'delete' && (
        <ConfirmDialog
          title={t('appearance.ui.deleteThisStyle')}
          body={[t('appearance.deleteBody')]}
          confirmLabel={t('appearance.deleteStyle')}
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
          title={t('appearance.ui.exportStyle')}
          body={[t('appearance.exportBody')]}
          confirmLabel={t('common.copyToClipboard')}
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
            aria-label={t('appearance.ui.exportedStyle')}
          />
        </ConfirmDialog>
      )}
      {dialog?.kind === 'import' && (
        <ConfirmDialog
          title={t('appearance.ui.importStyle')}
          body={[t('appearance.importBody')]}
          confirmLabel={t('common.import')}
          tone="neutral"
          confirmDisabled={!importText.trim()}
          onCancel={() => setDialog(null)}
          onConfirm={() => {
            let parsed: unknown;
            try {
              parsed = JSON.parse(importText);
            } catch {
              setImportError(t('common.invalidJson'));
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
            aria-label={t('appearance.ui.styleJson')}
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
  const { t } = useTranslation();
  const following = value === null;
  return (
    <div className="tune__field">
      <span className="tune__label">
        {label}
        {following ? (
          <span className="tune__badge">{t('nav.style')}</span>
        ) : (
          <button type="button" className="tune__reset" onClick={() => onChange(null)}>
            {t('appearance.useStyle')}
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
