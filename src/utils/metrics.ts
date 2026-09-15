import type { Availability, EngineState } from '@/types/metrics';

/** Display helpers for the metrics contract. Pure, and free of platform logic. */

/** Short label for the engine's overall state. */
export function formatEngineState(state: EngineState): string {
  return state === 'ready' ? 'Ready' : 'Empty';
}

/**
 * One-line explanation of an availability status.
 *
 * Deliberately preserves the distinction the backend took care to make: a
 * missing sensor, a permission problem and a transient glitch read differently
 * because the user can act on them differently.
 */
export function describeAvailability(availability: Availability): string {
  switch (availability.status) {
    case 'available':
      return 'Available';
    case 'unsupported':
      return `Not supported: ${availability.reason}`;
    case 'notDetected':
      return `Not detected: ${availability.reason}`;
    case 'permissionDenied':
      return `Permission required: ${availability.reason}`;
    case 'temporarilyUnavailable':
      return `Temporarily unavailable: ${availability.reason}`;
    case 'providerError':
      return `Provider error: ${availability.error.message}`;
    case 'notRegistered':
      return `Not registered: ${availability.reason}`;
  }
}

/** Whether the situation may resolve itself without the user doing anything. */
export function isTransient(availability: Availability): boolean {
  switch (availability.status) {
    case 'temporarilyUnavailable':
      return true;
    case 'providerError':
      return availability.error.recoverable;
    default:
      return false;
  }
}
