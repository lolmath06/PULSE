import type { ApplicationEntry, ProcessDetails, ProcessEntry } from '@/types/processes';
import { useProcessDetailsQuery } from '@/hooks/useProcessInspection';
import type { ActionTarget } from '@/hooks/useProcessActions';
import { ContextMenu } from '@/components/ProcessContextMenu/ContextMenu';
import type { MenuItem } from '@/components/ProcessContextMenu/ContextMenu';
import { processMenuItems } from '@/components/ProcessContextMenu/processMenuItems';
import type { ProcessMenuHandlers } from '@/components/ProcessContextMenu/processMenuItems';
import { searchTerms } from '@/utils/processes';
import { useTranslation } from 'react-i18next';

/** The context menu of one process row. Loads capabilities when it opens. */
export function ProcessContextMenu({
  entry,
  x,
  y,
  target,
  handlers,
  onClose,
}: {
  readonly entry: ProcessEntry;
  readonly x: number;
  readonly y: number;
  readonly target: (details: ProcessDetails | null) => ActionTarget;
  readonly handlers: ProcessMenuHandlers;
  readonly onClose: () => void;
}) {
  const { t } = useTranslation();
  const inspection = useProcessDetailsQuery(entry.instanceId);
  const items = processMenuItems(entry, inspection, target(inspection.details), handlers);

  return (
    <ContextMenu
      x={x}
      y={y}
      label={t('processes.menu.actionsFor', { name: entry.name })}
      items={items}
      onClose={onClose}
    />
  );
}

/**
 * The context menu of an application row.
 *
 * Deliberately **no** end/suspend/priority here: an application groups
 * processes that may belong to different users and hold different
 * permissions, so "end this application" would be an ambiguous, partial
 * action. Those live on individual process rows.
 */
export function ApplicationContextMenu({
  application,
  x,
  y,
  onInspect,
  onShowProcesses,
  onSearch,
  onClose,
}: {
  readonly application: ApplicationEntry;
  readonly x: number;
  readonly y: number;
  readonly onInspect: (application: ApplicationEntry) => void;
  readonly onShowProcesses: (application: ApplicationEntry) => void;
  readonly onSearch: (terms: readonly string[]) => void;
  readonly onClose: () => void;
}) {
  const { t } = useTranslation();
  const items: MenuItem[] = [
    {
      id: 'inspect',
      label: t('processes.menu.inspectApplication'),
      onSelect: () => onInspect(application),
    },
    {
      id: 'search',
      label: t('processes.menu.search'),
      onSelect: () => onSearch(searchTerms({ name: application.displayName })),
    },
    {
      id: 'show',
      label: t('processes.menu.showProcesses'),
      onSelect: () => onShowProcesses(application),
    },
  ];

  return (
    <ContextMenu
      x={x}
      y={y}
      label={t('processes.menu.actionsFor', { name: application.displayName })}
      items={items}
      onClose={onClose}
    />
  );
}
