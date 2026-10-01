import { useTranslation } from 'react-i18next';
import type { OverlayCapabilities } from '@/overlay/desktop';
import { CAPABILITY_KEYS } from '@/overlay/desktop';
import { capabilityLabel, capabilityStatusLabel, displayServerLabel } from '@/overlay/desktopText';

/**
 * Each overlay capability of this session, with its honest status and reason.
 * The reason is the backend's technical note (the API or protocol involved),
 * shown as written.
 */
export function CapabilityList({ capabilities }: { readonly capabilities: OverlayCapabilities }) {
  const { t } = useTranslation();
  return (
    <>
      <p className="card__muted">
        {t('overlays.session')} <strong>{displayServerLabel(capabilities.displayServer)}</strong>
      </p>
      <ul className="capabilities" aria-label={t('overlays.capabilityList')}>
        {CAPABILITY_KEYS.map((key) => {
          const capability = capabilities[key];
          return (
            <li key={key} className="capabilities__item">
              <span className={`capabilities__status capabilities__status--${capability.status}`}>
                {capabilityStatusLabel(capability.status)}
              </span>
              <span className="capabilities__label">{capabilityLabel(key)}</span>
              <span className="capabilities__reason" lang="en">
                {capability.reason}
              </span>
            </li>
          );
        })}
      </ul>
    </>
  );
}
