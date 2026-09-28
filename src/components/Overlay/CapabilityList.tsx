import type { OverlayCapabilities } from '@/overlay/desktop';
import { CAPABILITY_LABELS, DISPLAY_SERVER_LABELS } from '@/overlay/desktop';

/** Each overlay capability of this session, with its honest status and reason. */
export function CapabilityList({ capabilities }: { readonly capabilities: OverlayCapabilities }) {
  const entries = Object.entries(CAPABILITY_LABELS) as [keyof typeof CAPABILITY_LABELS, string][];
  return (
    <>
      <p className="card__muted">
        Session: <strong>{DISPLAY_SERVER_LABELS[capabilities.displayServer]}</strong>
      </p>
      <ul className="capabilities" aria-label="Overlay capabilities">
        {entries.map(([key, label]) => {
          const capability = capabilities[key];
          return (
            <li key={key} className="capabilities__item">
              <span className={`capabilities__status capabilities__status--${capability.status}`}>
                {capability.status === 'supported'
                  ? 'Supported'
                  : capability.status === 'limited'
                    ? 'Limited'
                    : 'Unsupported'}
              </span>
              <span className="capabilities__label">{label}</span>
              <span className="capabilities__reason">{capability.reason}</span>
            </li>
          );
        })}
      </ul>
    </>
  );
}
