import type {
  CapabilityStatus,
  GnomeBridgeStatus,
  OverlayBackendInfo,
  OverlayCapabilities,
} from '@/overlay/desktop';
import { hasKey, t } from '@/i18n/i18n';

/**
 * The desktop status, in the interface language.
 *
 * The backend describes overlay backends and the GNOME bridge in English next
 * to stable codes (`kind`, `state`, `summaryCode`, `guidanceCode`). Wording is
 * chosen from those codes only — never by reading the English — and the
 * backend's own sentence is shown when this PULSE does not know a code (a
 * newer backend). Per-capability reasons stay as the backend wrote them:
 * they cite the API or protocol involved (`HWND_TOPMOST`,
 * `wl_surface.set_input_region`) and the runtime error, which is diagnostic
 * detail worth keeping verbatim.
 */

export function backendLabel(info: Pick<OverlayBackendInfo, 'kind' | 'label'>): string {
  const key = `overlays.backends.${info.kind}.label`;
  return hasKey(key) ? t(key) : info.label;
}

export function backendDetail(info: Pick<OverlayBackendInfo, 'kind' | 'detail'>): string {
  const key = `overlays.backends.${info.kind}.detail`;
  return hasKey(key) ? t(key) : info.detail;
}

export function capabilityStatusLabel(status: CapabilityStatus): string {
  return t(`overlays.capabilityStatus.${status}`);
}

export function capabilityLabel(key: keyof Omit<OverlayCapabilities, 'displayServer'>): string {
  return t(`overlays.capabilities.${key}`);
}

export function displayServerLabel(server: OverlayCapabilities['displayServer']): string {
  return t(`overlays.displayServers.${server}`);
}

/** The bridge's one-line summary, from its code; the backend's text otherwise. */
export function bridgeSummary(
  bridge: Pick<GnomeBridgeStatus, 'summary' | 'shellVersion'> & { readonly summaryCode?: string },
): string {
  const key = bridge.summaryCode ? `overlays.bridge.summaries.${bridge.summaryCode}` : null;
  return key && hasKey(key) ? t(key, { version: bridge.shellVersion ?? '?' }) : bridge.summary;
}

/** The bridge's next step, from its code; the backend's text otherwise. */
export function bridgeGuidance(
  bridge: Pick<GnomeBridgeStatus, 'guidance'> & { readonly guidanceCode?: string | null },
): string | null {
  if (!bridge.guidance) return null;
  const key = bridge.guidanceCode ? `overlays.bridge.guidance.${bridge.guidanceCode}` : null;
  return key && hasKey(key) ? t(key) : bridge.guidance;
}
