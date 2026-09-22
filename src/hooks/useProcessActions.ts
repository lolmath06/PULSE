import { useCallback, useMemo, useRef, useState } from 'react';
import type { ProcessActionResult, ProcessPriority } from '@/types/processes';
import {
  computeProcessSha256,
  openHashLookup,
  openProcessLocation,
  openWebSearch,
  resumeProcess,
  setProcessAffinity,
  setProcessPriority,
  suspendProcess,
  terminateProcess,
  terminateProcessTree,
} from '@/services/processes';
import { copyText } from '@/utils/clipboard';

/** Who an action is aimed at: always an instance, never a bare PID. */
export interface ActionTarget {
  readonly instanceId: string;
  readonly pid: number;
  readonly name: string;
  readonly isSelf: boolean;
  /** Linux: End process sends SIGTERM and a separate Force kill exists. */
  readonly forceKillSupported: boolean;
  /** Approximate descendants, for the *End process tree* confirmation. */
  readonly descendants: number;
}

export type ProcessAction =
  | { readonly kind: 'suspend' }
  | { readonly kind: 'resume' }
  | { readonly kind: 'terminate' }
  | { readonly kind: 'forceKill' }
  | { readonly kind: 'terminateTree' }
  | { readonly kind: 'priority'; readonly priority: ProcessPriority }
  | { readonly kind: 'affinity'; readonly cpus: readonly number[] }
  | { readonly kind: 'openLocation' };

export interface ConfirmRequest {
  readonly title: string;
  readonly body: readonly string[];
  readonly confirmLabel: string;
  readonly tone: 'danger' | 'warning';
}

export interface Notice {
  readonly tone: 'success' | 'warning' | 'error';
  readonly text: string;
}

const STATUS_LABEL: Record<ProcessActionResult['status'], string> = {
  success: 'Done',
  permissionDenied: 'Permission denied',
  staleProcess: 'Not done — the PID now belongs to another process',
  processGone: 'Process already exited',
  unsupported: 'Unsupported',
  partialFailure: 'Partly done',
  invalidRequest: 'Not done',
  platformError: 'Failed',
};

/** Turns an action result into the sentence the card shows. */
export function noticeFor(result: ProcessActionResult): Notice {
  const tone: Notice['tone'] =
    result.status === 'success'
      ? 'success'
      : result.status === 'partialFailure' || result.status === 'processGone'
        ? 'warning'
        : 'error';
  const text =
    result.status === 'success'
      ? result.reason
      : `${STATUS_LABEL[result.status]}: ${result.reason}`;
  return { tone, text };
}

/**
 * The confirmation an action needs, or `null` when it runs immediately.
 *
 * Destructive actions always confirm, naming the process and its PID. Priority
 * changes do not — except Windows *Realtime*, which can freeze the machine.
 */
export function confirmationFor(
  action: ProcessAction,
  target: ActionTarget,
): ConfirmRequest | null {
  const who = `${target.name} (PID ${target.pid})`;
  const selfWarning = target.isSelf ? ['Ending PULSE will close this application.'] : [];

  switch (action.kind) {
    case 'terminate':
      return {
        title: `End ${target.name}?`,
        body: [
          target.forceKillSupported
            ? `${who} will be sent SIGTERM and asked to exit. It may take a moment, or refuse.`
            : `${who} will be terminated immediately. Unsaved work in it will be lost.`,
          ...selfWarning,
        ],
        confirmLabel: 'End process',
        tone: 'danger',
      };
    case 'forceKill':
      return {
        title: `Force kill ${target.name}?`,
        body: [
          `${who} will be sent SIGKILL. It cannot be caught or ignored: the process gets no ` +
            'chance to save anything or clean up.',
          ...selfWarning,
        ],
        confirmLabel: 'Force kill',
        tone: 'danger',
      };
    case 'terminateTree':
      return {
        title: `End ${target.name} and its process tree?`,
        body: [
          `Root process: ${who}.`,
          `About ${target.descendants} descendant process${target.descendants === 1 ? '' : 'es'} ` +
            'will be ended first, deepest first, then the root. Each one is re-checked just ' +
            'before it is ended; a PID that changed hands is skipped.',
          ...selfWarning,
        ],
        confirmLabel: 'End process tree',
        tone: 'danger',
      };
    case 'priority':
      if (action.priority.kind === 'windowsClass' && action.priority.class === 'realtime') {
        return {
          title: 'Set Realtime priority?',
          body: [
            'Realtime priority can make the system unresponsive.',
            `It applies to ${who}. Without the required privilege, Windows applies High ` +
              'instead, and PULSE will say so.',
          ],
          confirmLabel: 'Set Realtime',
          tone: 'warning',
        };
      }
      return null;
    default:
      return null;
  }
}

function perform(action: ProcessAction, instanceId: string): Promise<ProcessActionResult> {
  switch (action.kind) {
    case 'suspend':
      return suspendProcess(instanceId);
    case 'resume':
      return resumeProcess(instanceId);
    case 'terminate':
      return terminateProcess(instanceId, false);
    case 'forceKill':
      return terminateProcess(instanceId, true);
    case 'terminateTree':
      return terminateProcessTree(instanceId);
    case 'priority': {
      const realtime =
        action.priority.kind === 'windowsClass' && action.priority.class === 'realtime';
      return setProcessPriority(instanceId, action.priority, realtime);
    }
    case 'affinity':
      return setProcessAffinity(instanceId, action.cpus);
    case 'openLocation':
      return openProcessLocation(instanceId);
  }
}

function failure(error: unknown): ProcessActionResult {
  return {
    status: 'platformError',
    reason: error instanceof Error ? error.message : String(error),
    affectedCount: null,
    failedCount: null,
    tree: null,
  };
}

export interface ProcessActions {
  /** Starts an action — immediately, or by asking for confirmation first. */
  readonly run: (action: ProcessAction, target: ActionTarget) => void;
  readonly pending: ConfirmRequest | null;
  readonly confirm: () => void;
  readonly cancel: () => void;
  readonly busy: boolean;
  readonly notice: Notice | null;
  readonly dismissNotice: () => void;
  /** Bumped after every action, so an open inspector re-reads once. */
  readonly revision: number;
  readonly copy: (label: string, text: string) => void;
  readonly copyHash: (instanceId: string) => void;
  readonly searchOnline: (terms: readonly string[]) => void;
  readonly lookupHash: (sha256: string, target: 'web' | 'virusTotal') => void;
}

/**
 * Runs process actions on the user's behalf — and only on their behalf.
 *
 * Every entry point is a click. After a state-changing action the snapshot is
 * refreshed exactly once (`refresh`) and the inspector re-reads once
 * (`revision`); there is no timer and no polling here.
 */
export function useProcessActions(refresh: () => void): ProcessActions {
  const [pending, setPending] = useState<ConfirmRequest | null>(null);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState<Notice | null>(null);
  const [revision, setRevision] = useState(0);
  const queued = useRef<{ action: ProcessAction; target: ActionTarget } | null>(null);

  const execute = useCallback(
    (action: ProcessAction, target: ActionTarget) => {
      setBusy(true);
      perform(action, target.instanceId)
        .catch(failure)
        .then((result) => {
          setNotice(
            action.kind === 'openLocation' && result.status === 'success'
              ? null
              : noticeFor(result),
          );
          if (
            action.kind !== 'openLocation' &&
            (result.status === 'success' ||
              result.status === 'partialFailure' ||
              result.status === 'processGone')
          ) {
            refresh();
            setRevision((value) => value + 1);
          }
        })
        .finally(() => setBusy(false));
    },
    [refresh],
  );

  const run = useCallback(
    (action: ProcessAction, target: ActionTarget) => {
      const request = confirmationFor(action, target);
      if (request === null) {
        execute(action, target);
        return;
      }
      queued.current = { action, target };
      setPending(request);
    },
    [execute],
  );

  const confirm = useCallback(() => {
    const next = queued.current;
    queued.current = null;
    setPending(null);
    if (next !== null) execute(next.action, next.target);
  }, [execute]);

  const cancel = useCallback(() => {
    queued.current = null;
    setPending(null);
  }, []);

  const copy = useCallback((label: string, text: string) => {
    void copyText(text).then((copied) =>
      setNotice(
        copied
          ? { tone: 'success', text: `${label} copied.` }
          : { tone: 'error', text: `${label} could not be copied.` },
      ),
    );
  }, []);

  const copyHash = useCallback(
    (instanceId: string) => {
      setBusy(true);
      computeProcessSha256(instanceId)
        .then((query) => {
          const hash = query.value;
          if (hash?.status === 'computed' && hash.sha256 !== null) {
            copy('SHA-256', hash.sha256);
          } else {
            setNotice({
              tone: 'error',
              text: hash?.reason ?? noticeFor(query.outcome).text,
            });
          }
        })
        .catch((error: unknown) => setNotice(noticeFor(failure(error))))
        .finally(() => setBusy(false));
    },
    [copy],
  );

  const searchOnline = useCallback((terms: readonly string[]) => {
    void openWebSearch(terms)
      .catch(failure)
      .then((result) => setNotice(result.status === 'success' ? null : noticeFor(result)));
  }, []);

  const lookupHash = useCallback((sha256: string, target: 'web' | 'virusTotal') => {
    void openHashLookup(sha256, target)
      .catch(failure)
      .then((result) => setNotice(result.status === 'success' ? null : noticeFor(result)));
  }, []);

  const dismissNotice = useCallback(() => setNotice(null), []);

  return useMemo(
    () => ({
      run,
      pending,
      confirm,
      cancel,
      busy,
      notice,
      dismissNotice,
      revision,
      copy,
      copyHash,
      searchOnline,
      lookupHash,
    }),
    [
      run,
      pending,
      confirm,
      cancel,
      busy,
      notice,
      dismissNotice,
      revision,
      copy,
      copyHash,
      searchOnline,
      lookupHash,
    ],
  );
}
