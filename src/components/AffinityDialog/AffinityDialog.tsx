import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import type { ProcessAffinity } from '@/types/processes';
import { ConfirmDialog } from '@/components/ConfirmDialog/ConfirmDialog';

/**
 * Chooses which logical processors a process may run on.
 *
 * One checkbox per processor the machine offers. There is *Select all* but no
 * *Clear all*: a process allowed on no processor cannot run, so an empty set
 * is never offered, and *Apply* is disabled until at least one is checked.
 * The panel scrolls inside itself only past a few rows of processors.
 */
export function AffinityDialog({
  name,
  pid,
  affinity,
  onApply,
  onCancel,
}: {
  readonly name: string;
  readonly pid: number;
  readonly affinity: ProcessAffinity;
  readonly onApply: (cpus: number[]) => void;
  readonly onCancel: () => void;
}) {
  const { t } = useTranslation();
  const [selected, setSelected] = useState<ReadonlySet<number>>(new Set(affinity.cpus));
  const limited = affinity.limitation !== null;

  const toggle = (cpu: number) =>
    setSelected((current) => {
      const next = new Set(current);
      if (next.has(cpu)) next.delete(cpu);
      else next.add(cpu);
      return next;
    });

  return (
    <ConfirmDialog
      title={t('processes.affinity.title', { name })}
      body={[t('processes.affinity.body', { pid })]}
      confirmLabel={t('common.apply')}
      tone="neutral"
      confirmDisabled={selected.size === 0 || limited}
      onConfirm={() => onApply([...selected].sort((a, b) => a - b))}
      onCancel={onCancel}
    >
      {limited ? (
        <p className="dialog__text dialog__text--warning">{affinity.limitation}</p>
      ) : (
        <>
          <div
            className="affinity-grid"
            role="group"
            aria-label={t('metrics.catalog.cpu.count.logical.name')}
          >
            {affinity.available.map((cpu) => (
              <label key={cpu} className="affinity-grid__cpu">
                <input type="checkbox" checked={selected.has(cpu)} onChange={() => toggle(cpu)} />
                CPU {cpu}
              </label>
            ))}
          </div>
          <div className="affinity-tools">
            <button
              type="button"
              className="button button--quiet"
              onClick={() => setSelected(new Set(affinity.available))}
            >
              {t('processes.affinity.selectAll')}
            </button>
            {selected.size === 0 && (
              <span className="dialog__text--warning" role="alert">
                {t('processes.affinity.keepOne')}
              </span>
            )}
          </div>
        </>
      )}
    </ConfirmDialog>
  );
}
