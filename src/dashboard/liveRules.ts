/**
 * The frontend's copy of the live feed's refusals (`crate::live::live_refusal`),
 * so the library can say *before* adding a widget that a metric is read on
 * demand only. The backend remains the authority: it refuses these keys at
 * subscription time whatever the frontend says.
 */
export function liveRefusalReason(key: string): string | null {
  if (key.startsWith('storage.health.')) {
    return 'Read from the drive on demand (an NVMe admin command), never every second: it shows no live value.';
  }
  return null;
}
