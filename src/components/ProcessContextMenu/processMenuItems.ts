import type { Capability, ProcessDetails, ProcessEntry } from '@/types/processes';
import type { ProcessInspection } from '@/hooks/useProcessInspection';
import type { ActionTarget, ProcessAction } from '@/hooks/useProcessActions';
import type { MenuItem } from '@/components/ProcessContextMenu/ContextMenu';
import { NICE_PRESETS, WINDOWS_PRIORITY_CLASSES, searchTerms } from '@/utils/processes';

/** What the process menu can ask its owner to do. */
export interface ProcessMenuHandlers {
  readonly inspect: (entry: ProcessEntry) => void;
  readonly run: (action: ProcessAction, target: ActionTarget) => void;
  readonly copy: (label: string, text: string) => void;
  readonly copyHash: (instanceId: string) => void;
  readonly searchOnline: (terms: readonly string[]) => void;
  readonly customNice: (entry: ProcessEntry, details: ProcessDetails) => void;
  readonly affinity: (entry: ProcessEntry, details: ProcessDetails) => void;
}

const CHECKING: Capability = { allowed: false, reason: 'Checking permissions…' };

function gate(inspection: ProcessInspection, pick: (details: ProcessDetails) => Capability) {
  if (inspection.details !== null) return pick(inspection.details);
  if (inspection.status === 'failed') {
    return {
      allowed: false,
      reason:
        inspection.outcome?.status === 'processGone' ||
        inspection.outcome?.status === 'staleProcess'
          ? 'Process already exited.'
          : (inspection.outcome?.reason ?? 'Unavailable.'),
    };
  }
  return CHECKING;
}

/**
 * The items of a process row's context menu.
 *
 * Pure, so every enabled/disabled rule is tested without rendering a menu.
 * Destructive entries are enabled only once the inspector's capabilities have
 * arrived — and each disabled entry carries the backend's reason.
 */
export function processMenuItems(
  entry: ProcessEntry,
  inspection: ProcessInspection,
  target: ActionTarget,
  handlers: ProcessMenuHandlers,
): MenuItem[] {
  const details = inspection.details;
  const path = details?.executable.value?.path ?? entry.executablePath.value;
  const capability = (pick: (details: ProcessDetails) => Capability) => {
    const result = gate(inspection, pick);
    return { disabled: !result.allowed, reason: result.reason };
  };
  const run = (action: ProcessAction) => () => handlers.run(action, target);

  const priority = details?.priority.value ?? null;
  const priorityItems: MenuItem[] =
    details?.priorityKind === 'windowsClass'
      ? WINDOWS_PRIORITY_CLASSES.map((entry) => ({
          id: `priority-${entry.value}`,
          label: entry.label,
          checked: priority?.kind === 'windowsClass' && priority.class === entry.value,
          onSelect: run({
            kind: 'priority',
            priority: { kind: 'windowsClass', class: entry.value },
          }),
        }))
      : [
          ...NICE_PRESETS.map((preset) => ({
            id: `priority-${preset.value}`,
            label: `${preset.label} (nice ${preset.value})`,
            checked: priority?.kind === 'nice' && priority.value === preset.value,
            onSelect: run({ kind: 'priority', priority: { kind: 'nice', value: preset.value } }),
          })),
          {
            id: 'priority-custom',
            label: 'Custom nice value…',
            separated: true,
            onSelect: () => {
              if (details) handlers.customNice(entry, details);
            },
          },
        ];

  const forceKill: MenuItem[] =
    details === null || details.forceKillSupported
      ? [
          {
            id: 'force-kill',
            label: 'Force kill',
            tone: 'danger',
            ...capability((d) => d.capabilities.forceKill),
            onSelect: run({ kind: 'forceKill' }),
          },
        ]
      : [];

  return [
    { id: 'inspect', label: 'Inspect details', onSelect: () => handlers.inspect(entry) },
    {
      id: 'search',
      label: 'Search online',
      onSelect: () =>
        handlers.searchOnline(
          searchTerms({
            name: entry.name,
            executablePath: path,
            productName: details?.versionInfo.value?.productName ?? null,
          }),
        ),
    },
    {
      id: 'location',
      label: 'Open file location',
      ...capability((d) => d.capabilities.openLocation),
      onSelect: run({ kind: 'openLocation' }),
    },
    {
      id: 'copy',
      label: 'Copy',
      separated: true,
      submenu: [
        {
          id: 'copy-name',
          label: 'Process name',
          onSelect: () => handlers.copy('Process name', entry.name),
        },
        { id: 'copy-pid', label: 'PID', onSelect: () => handlers.copy('PID', String(entry.pid)) },
        {
          id: 'copy-path',
          label: 'Executable path',
          disabled: path === null,
          reason: path === null ? 'Executable unavailable.' : null,
          onSelect: () => {
            if (path !== null) handlers.copy('Executable path', path);
          },
        },
        {
          id: 'copy-instance',
          label: 'Process instance ID',
          onSelect: () => handlers.copy('Process instance ID', entry.instanceId),
        },
        {
          id: 'copy-sha256',
          label: 'SHA-256',
          ...capability((d) => d.capabilities.computeHash),
          onSelect: () => handlers.copyHash(entry.instanceId),
        },
      ],
    },
    {
      id: 'suspend',
      label: 'Suspend',
      separated: true,
      ...capability((d) => d.capabilities.suspend),
      onSelect: run({ kind: 'suspend' }),
    },
    {
      id: 'resume',
      label: 'Resume',
      ...capability((d) => d.capabilities.resume),
      onSelect: run({ kind: 'resume' }),
    },
    {
      id: 'end',
      label: 'End process',
      tone: 'danger',
      separated: true,
      ...capability((d) => d.capabilities.terminate),
      onSelect: run({ kind: 'terminate' }),
    },
    {
      id: 'end-tree',
      label: 'End process tree',
      tone: 'danger',
      ...capability((d) => d.capabilities.terminateTree),
      onSelect: run({ kind: 'terminateTree' }),
    },
    ...forceKill,
    {
      id: 'priority',
      label: 'Set priority',
      separated: true,
      ...capability((d) => d.capabilities.setPriority),
      submenu: priorityItems,
    },
    {
      id: 'affinity',
      label: 'Set affinity…',
      ...capability((d) =>
        d.affinity.value === null
          ? { allowed: false, reason: 'The current affinity could not be read.' }
          : d.capabilities.setAffinity,
      ),
      onSelect: () => {
        if (details) handlers.affinity(entry, details);
      },
    },
  ];
}
