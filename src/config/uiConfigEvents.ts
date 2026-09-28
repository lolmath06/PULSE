import { subscribeUiConfig } from '@/config/uiConfig';

/** Calls `listener` after any configuration change (local or from another window). */
export function onUiConfigChange(listener: () => void): () => void {
  return subscribeUiConfig(listener);
}
