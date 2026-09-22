import type { ApplicationEntry, ProcessDetails, ProcessEntry } from '@/types/processes';
import { useProcessDetailsQuery } from '@/hooks/useProcessInspection';
import type { ActionTarget } from '@/hooks/useProcessActions';
import { ContextMenu } from '@/components/ProcessContextMenu/ContextMenu';
import type { MenuItem } from '@/components/ProcessContextMenu/ContextMenu';
import { processMenuItems } from '@/components/ProcessContextMenu/processMenuItems';
import type { ProcessMenuHandlers } from '@/components/ProcessContextMenu/processMenuItems';
import { searchTerms } from '@/utils/processes';

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
  const inspection = useProcessDetailsQuery(entry.instanceId);
  const items = processMenuItems(entry, inspection, target(inspection.details), handlers);

  return (
    <ContextMenu x={x} y={y} label={`Actions for ${entry.name}`} items={items} onClose={onClose} />
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
  const items: MenuItem[] = [
    {
      id: 'inspect',
      label: 'Inspect application',
      onSelect: () => onInspect(application),
    },
    {
      id: 'search',
      label: 'Search online',
      onSelect: () => onSearch(searchTerms({ name: application.displayName })),
    },
    {
      id: 'show',
      label: 'Show processes',
      onSelect: () => onShowProcesses(application),
    },
  ];

  return (
    <ContextMenu
      x={x}
      y={y}
      label={`Actions for ${application.displayName}`}
      items={items}
      onClose={onClose}
    />
  );
}
