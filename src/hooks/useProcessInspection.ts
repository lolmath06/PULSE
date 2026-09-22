import { useEffect, useState } from 'react';
import type { ProcessActionResult, ProcessDetails, Provenance } from '@/types/processes';
import { getProcessDetails, getProcessProvenance } from '@/services/processes';

export type InspectionStatus = 'idle' | 'loading' | 'ready' | 'failed';

export interface ProcessInspection {
  readonly status: InspectionStatus;
  readonly details: ProcessDetails | null;
  /** Why `details` is missing: stale, gone, permission, unsupported… */
  readonly outcome: ProcessActionResult | null;
}

const IDLE: ProcessInspection = { status: 'idle', details: null, outcome: null };

function rejected(error: unknown): ProcessActionResult {
  return {
    status: 'platformError',
    reason: error instanceof Error ? error.message : String(error),
    affectedCount: null,
    failedCount: null,
    tree: null,
  };
}

/**
 * Loads the inspector's details for one instance, lazily.
 *
 * Nothing is requested until an instance is selected, and exactly one request
 * is made per selection and per `revision` — the owner bumps `revision` after
 * an action so the displayed priority, affinity and suspension state are
 * re-read once. No timer, no polling.
 *
 * While a re-read is in flight the previous details of the same instance stay
 * on screen, marked `loading`, rather than flashing empty.
 */
export function useProcessDetailsQuery(instanceId: string | null, revision = 0): ProcessInspection {
  const [answer, setAnswer] = useState<{
    readonly key: string;
    readonly result: ProcessInspection;
  } | null>(null);

  useEffect(() => {
    if (instanceId === null) return;
    const key = `${instanceId}#${revision}`;
    let cancelled = false;

    getProcessDetails(instanceId)
      .then((query) => {
        if (cancelled) return;
        setAnswer({
          key,
          result:
            query.value !== null
              ? { status: 'ready', details: query.value, outcome: null }
              : { status: 'failed', details: null, outcome: query.outcome },
        });
      })
      .catch((error: unknown) => {
        if (!cancelled) {
          setAnswer({ key, result: { status: 'failed', details: null, outcome: rejected(error) } });
        }
      });

    return () => {
      cancelled = true;
    };
  }, [instanceId, revision]);

  if (instanceId === null) return IDLE;
  if (answer?.key === `${instanceId}#${revision}`) return answer.result;
  if (answer?.result.details?.instanceId === instanceId) {
    return { ...answer.result, status: 'loading' };
  }
  return { status: 'loading', details: null, outcome: null };
}

export interface ProvenanceState {
  readonly status: InspectionStatus;
  readonly provenance: Provenance | null;
  readonly outcome: ProcessActionResult | null;
}

/**
 * Loads the package (Fedora) or signature (Windows) for one instance.
 *
 * Separate from the details because it can take a moment (`rpm -qf`,
 * `WinVerifyTrust`), and requested only once the inspector is open on that
 * instance. It never touches the network.
 */
export function useProcessProvenance(instanceId: string | null): ProvenanceState {
  const [answer, setAnswer] = useState<{
    readonly key: string;
    readonly result: ProvenanceState;
  } | null>(null);

  useEffect(() => {
    if (instanceId === null) return;
    let cancelled = false;

    getProcessProvenance(instanceId)
      .then((query) => {
        if (cancelled) return;
        setAnswer({
          key: instanceId,
          result:
            query.value !== null
              ? { status: 'ready', provenance: query.value, outcome: null }
              : { status: 'failed', provenance: null, outcome: query.outcome },
        });
      })
      .catch((error: unknown) => {
        if (!cancelled) {
          setAnswer({
            key: instanceId,
            result: { status: 'failed', provenance: null, outcome: rejected(error) },
          });
        }
      });

    return () => {
      cancelled = true;
    };
  }, [instanceId]);

  if (instanceId === null) return { status: 'idle', provenance: null, outcome: null };
  if (answer?.key === instanceId) return answer.result;
  return { status: 'loading', provenance: null, outcome: null };
}
