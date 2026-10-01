import { useEffect, useRef, useState } from 'react';
import type { ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { t } from '@/i18n/i18n';
import { formatDateTime } from '@/i18n/format';
import type {
  Capability,
  FileHash,
  ProcessActionResult,
  ProcessDetails,
  ProcessEntry,
  ProcessField,
  ProcessPriority,
  Provenance,
} from '@/types/processes';
import type { ActionTarget, ProcessAction } from '@/hooks/useProcessActions';
import { useProcessDetailsQuery, useProcessProvenance } from '@/hooks/useProcessInspection';
import { computeProcessSha256 } from '@/services/processes';
import { describeAvailability } from '@/utils/metrics';
import {
  NICE_PRESETS,
  WINDOWS_PRIORITY_CLASSES,
  priorityLabel,
  describeProvenance,
  formatAffinity,
  formatCategory,
  formatPackage,
  formatPriority,
  formatProcessState,
  formatTrust,
  searchTerms,
} from '@/utils/processes';
import { formatBytes, formatCount, formatPercent, formatThroughput } from '@/utils/units';

type HashState =
  | { readonly status: 'idle' }
  | { readonly status: 'computing' }
  | { readonly status: 'done'; readonly hash: FileHash }
  | { readonly status: 'failed'; readonly outcome: ProcessActionResult };

export interface InspectorHandlers {
  readonly run: (action: ProcessAction, target: ActionTarget) => void;
  readonly copy: (label: string, text: string) => void;
  readonly searchOnline: (terms: readonly string[]) => void;
  readonly lookupHash: (sha256: string, target: 'web' | 'virusTotal') => void;
  readonly customNice: (details: ProcessDetails) => void;
  readonly affinity: (details: ProcessDetails) => void;
}

/**
 * Everything PULSE can say about one process, beside the table.
 *
 * # Lazy
 *
 * Details are read when the inspector opens; the package or signature right
 * after, in a separate request because it can take a moment; the SHA-256 only
 * when *Compute SHA-256* is clicked. Nothing here is read on Refresh, and
 * nothing here touches the network — only the three explicit *Search* /
 * *VirusTotal* buttons open the browser.
 *
 * The owner mounts one inspector per selected instance (`key`), so the hash
 * of one process can never be shown beside another.
 *
 * # Provenance is not a verdict
 *
 * A package, a signature and a hash say where a file came from. The inspector
 * never labels a process safe or malicious.
 */
export function ProcessInspector({
  instanceId,
  entry,
  fallbackName,
  revision,
  busy,
  context,
  target,
  handlers,
  onClose,
}: {
  readonly instanceId: string;
  /** The row from the latest snapshot, or `null` once the process has exited. */
  readonly entry: ProcessEntry | null;
  readonly fallbackName: string;
  readonly revision: number;
  readonly busy: boolean;
  /** Set when opened from an application row. */
  readonly context?: string;
  readonly target: (details: ProcessDetails | null) => ActionTarget;
  readonly handlers: InspectorHandlers;
  readonly onClose: () => void;
}) {
  // Subscribes this panel (and its rows) to language changes.
  useTranslation();
  const inspection = useProcessDetailsQuery(instanceId, revision);
  const provenance = useProcessProvenance(inspection.details === null ? null : instanceId);
  const [hash, setHash] = useState<HashState>({ status: 'idle' });
  const panel = useRef<HTMLElement>(null);

  useEffect(() => {
    panel.current?.focus();
  }, [instanceId]);

  const details = inspection.details;
  const exited =
    entry === null ||
    inspection.outcome?.status === 'processGone' ||
    inspection.outcome?.status === 'staleProcess';
  const name = details?.name ?? entry?.name ?? fallbackName;
  const product = details?.versionInfo.value?.productName ?? null;
  const actionTarget = target(details);
  const run = (action: ProcessAction) => handlers.run(action, actionTarget);

  const computeHash = () => {
    setHash({ status: 'computing' });
    computeProcessSha256(instanceId)
      .then((query) =>
        setHash(
          query.value !== null
            ? { status: 'done', hash: query.value }
            : { status: 'failed', outcome: query.outcome },
        ),
      )
      .catch((error: unknown) =>
        setHash({
          status: 'failed',
          outcome: {
            status: 'platformError',
            reason: error instanceof Error ? error.message : String(error),
            affectedCount: null,
            failedCount: null,
            tree: null,
          },
        }),
      );
  };

  const terms = searchTerms({
    name,
    executablePath: details?.executable.value?.path ?? entry?.executablePath.value ?? null,
    productName: product,
    publisher:
      provenance.provenance?.kind === 'signature'
        ? provenance.provenance.signature.publisher
        : null,
    packageName:
      provenance.provenance?.kind === 'rpmPackage'
        ? (provenance.provenance.packages[0]?.name ?? null)
        : null,
  });

  const allowed = (capability: Capability | undefined) =>
    !exited && !busy && capability?.allowed === true;

  return (
    <aside
      ref={panel}
      className="inspector"
      aria-label={t('processes.inspector.processInspector')}
      tabIndex={-1}
      onKeyDown={(event) => {
        if (event.key === 'Escape') {
          event.stopPropagation();
          onClose();
        }
      }}
    >
      <header className="inspector__header">
        <div className="inspector__heading">
          <h3 className="inspector__title" title={name}>
            {product ?? name}
          </h3>
          <span className="inspector__subtitle">
            {product !== null && product !== name ? `${name} · ` : ''}PID{' '}
            {details?.pid ?? entry?.pid ?? '—'}
            {context ? ` · ${context}` : ''}
          </span>
        </div>
        <button
          type="button"
          className="inspector__close"
          aria-label={t('processes.inspector.closeInspector')}
          onClick={onClose}
        >
          ×
        </button>
      </header>

      {exited && (
        <p className="inspector__banner inspector__banner--gone" role="status">
          {inspection.outcome?.status === 'staleProcess'
            ? t('processes.inspector.stale')
            : t('processes.inspector.exited')}
        </p>
      )}

      {inspection.status === 'loading' && details === null && (
        <p className="card__muted">{t('processes.inspector.reading')}</p>
      )}

      {inspection.status === 'failed' && !exited && inspection.outcome && (
        <p className="inspector__banner" role="status">
          {`${t(`processes.status.${inspection.outcome.status}`)}: ${inspection.outcome.reason}`}
        </p>
      )}

      {details?.isSelf && (
        <p className="inspector__banner inspector__banner--warning">
          {t('processes.inspector.self')}
        </p>
      )}

      {details?.category === 'kernelThread' && (
        <p className="inspector__banner">{t('processes.inspector.kernelThread')}</p>
      )}

      <div className="inspector__actions">
        <button
          type="button"
          className="button button--quiet"
          onClick={() => handlers.searchOnline(terms)}
          title={t('processes.inspector.searchHint')}
        >
          {t('processes.menu.search')}
        </button>
        <button
          type="button"
          className="button button--quiet"
          disabled={!allowed(details?.capabilities.openLocation)}
          title={details?.capabilities.openLocation.reason ?? undefined}
          onClick={() => run({ kind: 'openLocation' })}
        >
          {t('processes.menu.openLocation')}
        </button>
      </div>

      <Section title={t('processes.inspector.identity')}>
        <Row label={t('processes.inspector.processName')}>{name}</Row>
        {product !== null && product !== name && (
          <Row label={t('processes.inspector.product')}>{product}</Row>
        )}
        <Row label="PID">{details?.pid ?? entry?.pid ?? '—'}</Row>
        <Row label={t('processes.inspector.parent')}>
          {details?.parentPid == null ? (
            '—'
          ) : (
            <>
              <FieldText field={details.parentName} /> · PID {details.parentPid}
            </>
          )}
        </Row>
        <Row label={t('processes.inspector.instanceId')}>
          <code className="inspector__mono">{instanceId}</code>
        </Row>
        <Row label={t('processes.inspector.state')}>
          {details ? stateLabel(details) : entry ? formatProcessState(entry.state) : '—'}
        </Row>
        <Row label={t('processes.inspector.started')}>
          <FieldText field={details?.startedAt} format={(ms) => formatDateTime(ms)} />
        </Row>
        <Row label={t('processes.inspector.owner')}>
          <FieldText
            field={details?.owner}
            format={(owner) => (owner.name ? `${owner.name} (${owner.id})` : owner.id)}
          />
        </Row>
        <Row label={t('processes.inspector.category')}>
          {details ? formatCategory(details.category) : '—'}
        </Row>
        <Row label={t('processes.inspector.architecture')}>
          <FieldText field={details?.architecture} />
        </Row>
      </Section>

      <Section title={t('processes.inspector.resourceUsage')}>
        {entry === null ? (
          <p className="card__muted">{t('processes.inspector.notInSnapshot')}</p>
        ) : (
          <>
            <Row label="CPU">
              <FieldText field={entry.cpuPercent} format={(value) => formatPercent(value)} />
            </Row>
            <Row label={t('processes.inspector.memory')}>
              <FieldText field={entry.residentMemoryBytes} format={(value) => formatBytes(value)} />
            </Row>
            <Row label={t('processes.inspector.memoryPercent')}>
              <FieldText field={entry.memoryPercent} format={(value) => formatPercent(value)} />
            </Row>
            <Row label={t('processes.inspector.threads')}>
              <FieldText field={entry.threadCount} format={formatCount} />
            </Row>
            <Row label={t('processes.inspector.read')}>
              <FieldText field={entry.readBytesPerSecond} format={formatThroughput} />
            </Row>
            <Row label={t('processes.inspector.write')}>
              <FieldText field={entry.writeBytesPerSecond} format={formatThroughput} />
            </Row>
          </>
        )}
      </Section>

      <Section title={t('processes.inspector.executable')}>
        {details?.executable.value ? (
          <>
            <Row label={t('processes.inspector.path')}>
              <code className="inspector__mono inspector__path">
                {details.executable.value.path}
              </code>
            </Row>
            <Row label={t('processes.inspector.file')}>{details.executable.value.fileName}</Row>
            <Row label={t('processes.inspector.size')}>
              <FieldText
                field={details.executable.value.sizeBytes}
                format={(bytes) => formatBytes(bytes)}
              />
            </Row>
            <Row label={t('processes.inspector.modified')}>
              <FieldText
                field={details.executable.value.modifiedAt}
                format={(ms) => formatDateTime(ms)}
              />
            </Row>
            {details.executable.value.replacedOnDisk && (
              <p className="inspector__banner inspector__banner--warning">
                {t('processes.inspector.replaced')}
              </p>
            )}
            <div className="inspector__inline-actions">
              <button
                type="button"
                className="button button--quiet"
                onClick={() => {
                  const path = details.executable.value?.path;
                  if (path) handlers.copy(t('processes.menu.executablePath'), path);
                }}
              >
                {t('processes.inspector.copyPath')}
              </button>
            </div>
          </>
        ) : (
          <p className="card__muted">
            {details ? describeAvailability(details.executable.availability) : '—'}
          </p>
        )}
      </Section>

      <Section title={t('processes.inspector.securityProvenance')}>
        <ProvenanceRows state={provenance} details={details} />
        {details?.versionInfo.value && (
          <>
            <Row label={t('processes.inspector.description')}>
              {details.versionInfo.value.fileDescription ?? '—'}
            </Row>
            <Row label={t('processes.inspector.company')}>
              {details.versionInfo.value.companyName ?? '—'}
            </Row>
            <Row label={t('processes.inspector.version')}>
              {details.versionInfo.value.productVersion ??
                details.versionInfo.value.fileVersion ??
                '—'}
            </Row>
            <p className="inspector__note">{t('processes.inspector.versionNote')}</p>
          </>
        )}

        <Row label="SHA-256">
          <HashView state={hash} />
        </Row>
        <div className="inspector__inline-actions">
          {hash.status !== 'done' || hash.hash.status !== 'computed' ? (
            <button
              type="button"
              className="button button--quiet"
              disabled={!allowed(details?.capabilities.computeHash) || hash.status === 'computing'}
              title={details?.capabilities.computeHash.reason ?? undefined}
              onClick={computeHash}
            >
              {hash.status === 'computing'
                ? t('processes.inspector.computing')
                : t('processes.inspector.computeHash')}
            </button>
          ) : (
            <HashActions sha256={hash.hash.sha256 ?? ''} handlers={handlers} />
          )}
        </div>
        <p className="inspector__note">{t('processes.inspector.provenanceNote')}</p>
      </Section>

      <Section title={t('processes.inspector.scheduling')}>
        <Row label={t('processes.inspector.priority')}>
          <FieldText field={details?.priority} format={formatPriority} />
        </Row>
        {details && (
          <PriorityControl
            details={details}
            disabled={!allowed(details.capabilities.setPriority)}
            onSet={(priority) => run({ kind: 'priority', priority })}
            onCustom={() => handlers.customNice(details)}
          />
        )}
        <Row label={t('processes.inspector.cpuAffinity')}>
          <FieldText field={details?.affinity} format={formatAffinity} />
        </Row>
        {details && (
          <div className="inspector__inline-actions">
            <button
              type="button"
              className="button button--quiet"
              disabled={
                !allowed(details.capabilities.setAffinity) || details.affinity.value === null
              }
              title={details.capabilities.setAffinity.reason ?? undefined}
              onClick={() => handlers.affinity(details)}
            >
              {t('processes.inspector.changeAffinity')}
            </button>
          </div>
        )}
      </Section>

      <Section title={t('processes.inspector.processControl')} tone="danger">
        <div className="inspector__control-grid">
          <ControlButton
            label={t('processes.inspector.suspend')}
            capability={details?.capabilities.suspend}
            enabled={allowed(details?.capabilities.suspend)}
            onClick={() => run({ kind: 'suspend' })}
          />
          <ControlButton
            label={t('processes.inspector.resume')}
            capability={details?.capabilities.resume}
            enabled={allowed(details?.capabilities.resume)}
            onClick={() => run({ kind: 'resume' })}
          />
          <ControlButton
            label={t('processes.inspector.endProcess')}
            danger
            capability={details?.capabilities.terminate}
            enabled={allowed(details?.capabilities.terminate)}
            onClick={() => run({ kind: 'terminate' })}
          />
          <ControlButton
            label={t('processes.inspector.endProcessTree')}
            danger
            capability={details?.capabilities.terminateTree}
            enabled={allowed(details?.capabilities.terminateTree)}
            onClick={() => run({ kind: 'terminateTree' })}
          />
          {details?.forceKillSupported && (
            <ControlButton
              label={t('processes.inspector.forceKill')}
              danger
              capability={details.capabilities.forceKill}
              enabled={allowed(details.capabilities.forceKill)}
              onClick={() => run({ kind: 'forceKill' })}
            />
          )}
        </div>
        {details?.suspendedByPulse && (
          <p className="inspector__note">{t('processes.inspector.suspendedByPulse')}</p>
        )}
      </Section>
    </aside>
  );
}

function stateLabel(details: ProcessDetails): string {
  if (details.state === 'other') return describeAvailability(details.stateAvailability);
  const label = formatProcessState(details.state);
  return details.suspendedByPulse
    ? t('processes.inspector.suspendedState', { state: label })
    : label;
}

function Section({
  title,
  tone,
  children,
}: {
  readonly title: string;
  readonly tone?: 'danger';
  readonly children: ReactNode;
}) {
  return (
    <section
      className={tone ? `inspector__section inspector__section--${tone}` : 'inspector__section'}
      aria-label={title}
    >
      <h4 className="inspector__section-title">{title}</h4>
      {children}
    </section>
  );
}

function Row({ label, children }: { readonly label: string; readonly children: ReactNode }) {
  return (
    <div className="inspector__row">
      <span className="inspector__label">{label}</span>
      <span className="inspector__value">{children}</span>
    </div>
  );
}

/** A field's value, or a dash with the backend's reason beside it. */
function FieldText<T>({
  field,
  format,
}: {
  readonly field: ProcessField<T> | undefined;
  readonly format?: (value: T) => string;
}) {
  if (field === undefined) return <>—</>;
  if (field.value === null) {
    return (
      <span className="value--unavailable" title={describeAvailability(field.availability)}>
        — <span className="inspector__reason">{describeAvailability(field.availability)}</span>
      </span>
    );
  }
  return <>{format ? format(field.value) : String(field.value)}</>;
}

function ProvenanceRows({
  state,
  details,
}: {
  readonly state: ReturnType<typeof useProcessProvenance>;
  readonly details: ProcessDetails | null;
}) {
  if (details?.category === 'kernelThread') {
    return (
      <Row label={t('processes.inspector.provenance')}>
        {t('processes.inspector.notApplicableKernel')}
      </Row>
    );
  }
  if (state.status === 'loading')
    return <Row label={t('processes.inspector.provenance')}>{t('common.checking')}</Row>;
  if (state.status !== 'ready' || state.provenance === null) {
    return (
      <Row label={t('processes.inspector.provenance')}>
        {state.outcome?.reason ?? t('processes.trust.unavailable')}
      </Row>
    );
  }
  return <ProvenanceDetail provenance={state.provenance} />;
}

function ProvenanceDetail({ provenance }: { readonly provenance: Provenance }) {
  switch (provenance.kind) {
    case 'rpmPackage':
      return (
        <Row label={t('processes.inspector.package')}>
          {provenance.packages.map((pkg) => (
            <code key={formatPackage(pkg)} className="inspector__mono inspector__package">
              {formatPackage(pkg)}
            </code>
          ))}
        </Row>
      );
    case 'signature':
      return (
        <>
          <Row label={t('processes.inspector.signature')}>
            <span className={`trust trust--${provenance.signature.trust}`}>
              {formatTrust(provenance.signature.trust)}
            </span>
            {provenance.signature.source === 'catalog'
              ? ` ${t('processes.inspector.catalog')}`
              : ''}
          </Row>
          <Row label={t('processes.inspector.publisher')}>
            {provenance.signature.publisher ?? '—'}
            {provenance.signature.publisher !== null &&
              provenance.signature.trust !== 'trusted' &&
              ` ${t('processes.inspector.claimed')}`}
          </Row>
          <p className="inspector__note" lang="en">
            {provenance.signature.detail}
          </p>
        </>
      );
    default:
      return <Row label={t('processes.inspector.package')}>{describeProvenance(provenance)}</Row>;
  }
}

function HashView({ state }: { readonly state: HashState }) {
  switch (state.status) {
    case 'idle':
      return <span className="card__muted">{t('processes.inspector.notComputed')}</span>;
    case 'computing':
      return <span className="card__muted">{t('processes.inspector.computing')}</span>;
    case 'failed':
      return (
        <span className="value--unavailable">
          {`${t(`processes.status.${state.outcome.status}`)}: ${state.outcome.reason}`}
        </span>
      );
    default:
      if (state.hash.status === 'computed' && state.hash.sha256 !== null) {
        return <code className="inspector__mono inspector__hash">{state.hash.sha256}</code>;
      }
      return (
        <span className="value--unavailable">
          {state.hash.reason
            ? `${t(`processes.hash.${state.hash.status}`)}: ${state.hash.reason}`
            : t(`processes.hash.${state.hash.status}`)}
        </span>
      );
  }
}

function HashActions({
  sha256,
  handlers,
}: {
  readonly sha256: string;
  readonly handlers: InspectorHandlers;
}) {
  return (
    <>
      <button
        type="button"
        className="button button--quiet"
        onClick={() => handlers.copy('SHA-256', sha256)}
      >
        {t('processes.inspector.copyHash')}
      </button>
      <button
        type="button"
        className="button button--quiet"
        onClick={() => handlers.lookupHash(sha256, 'web')}
      >
        {t('processes.inspector.searchHash')}
      </button>
      <button
        type="button"
        className="button button--quiet"
        title={t('processes.inspector.virusTotalHint')}
        onClick={() => handlers.lookupHash(sha256, 'virusTotal')}
      >
        {t('processes.inspector.checkVirusTotal')}
      </button>
    </>
  );
}

function PriorityControl({
  details,
  disabled,
  onSet,
  onCustom,
}: {
  readonly details: ProcessDetails;
  readonly disabled: boolean;
  readonly onSet: (priority: ProcessPriority) => void;
  readonly onCustom: () => void;
}) {
  const current = details.priority.value;

  if (details.priorityKind === 'windowsClass') {
    const value = current?.kind === 'windowsClass' ? current.class : '';
    return (
      <label className="inspector__select">
        <span className="inspector__label">{t('processes.inspector.setPriority')}</span>
        <select
          value={value}
          disabled={disabled}
          aria-label={t('processes.inspector.setPriority')}
          onChange={(event) => {
            const chosen = WINDOWS_PRIORITY_CLASSES.find(
              (entry) => entry.value === event.target.value,
            );
            if (chosen) onSet({ kind: 'windowsClass', class: chosen.value });
          }}
        >
          {value === '' && <option value="">—</option>}
          {WINDOWS_PRIORITY_CLASSES.map((entry) => (
            <option key={entry.value} value={entry.value}>
              {priorityLabel(entry.level)}
            </option>
          ))}
        </select>
      </label>
    );
  }

  const value = current?.kind === 'nice' ? current.value : null;
  const preset = NICE_PRESETS.find((entry) => entry.value === value);
  return (
    <div className="inspector__select">
      <span className="inspector__label">{t('processes.inspector.setPriority')}</span>
      <select
        value={preset ? String(preset.value) : 'custom'}
        disabled={disabled}
        aria-label={t('processes.inspector.setPriority')}
        onChange={(event) => {
          if (event.target.value === 'custom') {
            onCustom();
            return;
          }
          onSet({ kind: 'nice', value: Number(event.target.value) });
        }}
      >
        {!preset && <option value="custom">{value === null ? '—' : `nice ${value}`}</option>}
        {NICE_PRESETS.map((entry) => (
          <option key={entry.value} value={String(entry.value)}>
            {t('processes.menu.nicePreset', {
              level: priorityLabel(entry.level),
              value: entry.value,
            })}
          </option>
        ))}
      </select>
      <button type="button" className="button button--quiet" disabled={disabled} onClick={onCustom}>
        {t('processes.inspector.custom')}
      </button>
    </div>
  );
}

function ControlButton({
  label,
  capability,
  enabled,
  danger = false,
  onClick,
}: {
  readonly label: string;
  readonly capability: Capability | undefined;
  readonly enabled: boolean;
  readonly danger?: boolean;
  readonly onClick: () => void;
}) {
  const reason = capability?.allowed === false ? capability.reason : null;
  return (
    <div className="inspector__control">
      <button
        type="button"
        className={danger ? 'button button--danger-quiet' : 'button button--quiet'}
        disabled={!enabled}
        onClick={onClick}
      >
        {label}
      </button>
      {reason && <span className="inspector__reason">{reason}</span>}
    </div>
  );
}
