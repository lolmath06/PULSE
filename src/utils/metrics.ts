import type { Availability, EngineState } from '@/types/metrics';
import { t } from '@/i18n/i18n';

/** Display helpers for the metrics contract. Pure, and free of platform logic. */

/** Short label for the engine's overall state. */
export function formatEngineState(state: EngineState): string {
  return t(`metrics.engineState.${state}`);
}

/**
 * One-line explanation of an availability status.
 *
 * Deliberately preserves the distinction the backend took care to make: a
 * missing sensor, a permission problem and a transient glitch read differently
 * because the user can act on them differently.
 *
 * The status words are translated (they come from the status code, never from
 * the text); the backend's reason follows verbatim, as technical detail.
 */
export function describeAvailability(availability: Availability): string {
  switch (availability.status) {
    case 'available':
      return t('metrics.availability.available');
    case 'providerError':
      return t('metrics.availabilityReason.providerError', {
        reason: availability.error.message,
      });
    default:
      return t(`metrics.availabilityReason.${availability.status}`, {
        reason: availability.reason,
      });
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
