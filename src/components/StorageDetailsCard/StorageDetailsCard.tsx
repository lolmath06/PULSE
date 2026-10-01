import type { MetricSample } from '@/types/metrics';
import { Trans, useTranslation } from 'react-i18next';
import { t } from '@/i18n/i18n';
import { formatFixed } from '@/i18n/format';
import {
  STORAGE_CAPACITY_TOTAL_KEY,
  STORAGE_DEVICE_COUNT,
  STORAGE_HEALTH_AVAILABLE_SPARE_KEY,
  STORAGE_HEALTH_MEDIA_ERRORS_KEY,
  STORAGE_HEALTH_PERCENTAGE_USED_KEY,
  STORAGE_HEALTH_POWER_ON_HOURS_KEY,
  STORAGE_HEALTH_TEMPERATURE_KEY,
  STORAGE_HEALTH_UNSAFE_SHUTDOWNS_KEY,
  STORAGE_IO_READ_BYTES_KEY,
  STORAGE_IO_READ_IOPS_KEY,
  STORAGE_IO_READ_LATENCY_KEY,
  STORAGE_IO_WRITE_BYTES_KEY,
  STORAGE_IO_WRITE_IOPS_KEY,
  STORAGE_IO_WRITE_LATENCY_KEY,
  STORAGE_VOLUME_CAPACITY_AVAILABLE_KEY,
  STORAGE_VOLUME_CAPACITY_TOTAL_KEY,
  STORAGE_VOLUME_CAPACITY_USED_KEY,
  STORAGE_VOLUME_COUNT,
  STORAGE_VOLUME_USAGE_PERCENT_KEY,
  metricRefId,
} from '@/types/wellknown';
import { useStorageDetails } from '@/hooks/useStorageDetails';
import type { StorageDevice, StorageVolume } from '@/utils/storage';
import {
  disambiguateStorageLabels,
  storageMetric,
  storageTelemetryState,
  volumesByDevice,
} from '@/utils/storage';
import { describeAvailability } from '@/utils/metrics';
import {
  formatBytes,
  formatBytesScaledTo,
  formatCelsius,
  formatCount,
  formatHours,
  formatIops,
  formatLatency,
  formatPercent,
  formatSampleTime,
  formatThroughput,
} from '@/utils/units';

/**
 * Real storage inventory, activity and health, sampled on demand.
 *
 * Every number comes from the backend. Nothing is fabricated: a disk whose
 * controller will not report its health is shown, named and identified, with
 * `—` and a reason on each value it cannot provide — never `0 °C` or `0 %`,
 * which a user would read as measurements.
 *
 * The inventory is **discovered from the catalog**, so this component contains
 * no list of disks and works unchanged from zero devices to four.
 *
 * # Devices and volumes are two different things
 *
 * A disk is hardware; a volume is a filesystem living on some of it. They do
 * not line up one to one — one disk holds several volumes, one volume can span
 * disks, and one filesystem is often reachable at several paths at once. The
 * card shows volumes nested under the disk the **backend** attributed them to,
 * and a volume it could not attribute goes under "Other volumes" rather than
 * being attached to whichever disk looks plausible.
 */
export function StorageDetailsCard() {
  useTranslation();
  const { status, devices, volumes, samples, message, refreshing, refresh } = useStorageDetails();

  const countOf = (id: string, fallback: number): number => {
    const sample = samples.get(id);
    return sample?.value?.type === 'number' ? sample.value.value : fallback;
  };

  const deviceCount = countOf(metricRefId(STORAGE_DEVICE_COUNT), devices.length);
  const volumeCount = countOf(metricRefId(STORAGE_VOLUME_COUNT), volumes.length);

  const labels = disambiguateStorageLabels(devices);
  const { byDevice, unattributed } = volumesByDevice(devices, volumes);
  const timestamp = samples.values().next().value?.timestamp;

  return (
    <div className="card" aria-label={t('cards.storage.storageDetails')}>
      <h2 className="card__title">{t('cards.storage.storageDetails')}</h2>

      {status === 'loading' && <p className="card__muted">{t('cards.storage.loading')}</p>}

      {status === 'error' && (
        <p className="card__muted" title={message}>
          <Trans i18nKey="overview.backendUnavailable" components={{ code: <code /> }} />
        </p>
      )}

      {status === 'ready' && (
        <>
          <dl className="kv">
            <div className="kv__row">
              <dt>{t('cards.storage.storageDevices')}</dt>
              <dd>{deviceCount}</dd>
            </div>
            <div className="kv__row">
              <dt>{t('metrics.catalog.storage.volume.count.name')}</dt>
              <dd>{volumeCount}</dd>
            </div>
          </dl>

          {devices.length === 0 ? (
            <p className="card__note">{t('cards.storage.none')}</p>
          ) : (
            <ul className="storage-grid" aria-label={t('cards.storage.storageDevices')}>
              {devices.map((device, index) => (
                <StorageEntry
                  key={device.sourceId}
                  device={device}
                  label={labels[index] ?? device.label}
                  volumes={byDevice.get(device.sourceId) ?? []}
                  samples={samples}
                />
              ))}
            </ul>
          )}

          {unattributed.length > 0 && (
            <section className="storage-other" aria-label={t('cards.storage.otherVolumes')}>
              <h3 className="storage-other__title">{t('cards.storage.otherVolumes')}</h3>
              <p className="card__muted">{t('cards.storage.otherVolumesHint')}</p>
              <ul className="storage-volumes">
                {unattributed.map((volume) => (
                  <VolumeEntry key={volume.sourceId} volume={volume} samples={samples} />
                ))}
              </ul>
            </section>
          )}

          <div className="card__footer">
            <span className="card__muted">
              {typeof timestamp === 'number'
                ? t('cards.updated', { time: formatSampleTime(timestamp) })
                : ''}
            </span>
            <button
              type="button"
              className="button"
              onClick={refresh}
              disabled={refreshing}
              aria-label={t('cards.storage.refreshStorageDetails')}
            >
              {refreshing ? t('common.refreshing') : t('common.refresh')}
            </button>
          </div>

          <p className="card__note">
            <Trans i18nKey="cards.storage.note" components={{ em: <em />, code: <code /> }} />
          </p>
        </>
      )}
    </div>
  );
}

/** One physical device: its name, its activity, its health, then its volumes. */
function StorageEntry({
  device,
  label,
  volumes,
  samples,
}: {
  readonly device: StorageDevice;
  readonly label: string;
  readonly volumes: readonly StorageVolume[];
  readonly samples: ReadonlyMap<string, MetricSample>;
}) {
  const sampleOf = (key: string): MetricSample | undefined =>
    samples.get(metricRefId(storageMetric(device.sourceId, key)));

  const numberOf = (key: string): number | null => {
    const sample = sampleOf(key);
    if (!sample || sample.value === null || sample.value.type !== 'number') return null;
    return sample.value.value;
  };

  const capacity = numberOf(STORAGE_CAPACITY_TOTAL_KEY);
  const telemetry = storageTelemetryState(samples, device.sourceId);

  return (
    <li className="storage-grid__item">
      <h3 className="storage-grid__name" title={device.sourceId}>
        {label}
      </h3>
      <p className="storage-grid__capacity">
        {capacity !== null ? (
          formatBytes(capacity)
        ) : (
          <Unavailable sample={sampleOf(STORAGE_CAPACITY_TOTAL_KEY)} />
        )}
      </p>

      {telemetry.ioAwaitingBaseline && (
        <p className="storage-grid__notice" role="note">
          <strong>{t('cards.waitingSample')}</strong>
          {t('cards.storage.waitingHint')}
        </p>
      )}

      <dl className="kv kv--compact">
        <Row label={t('cards.storage.read')}>
          <Throughput sample={sampleOf(STORAGE_IO_READ_BYTES_KEY)} />
        </Row>
        <Row label={t('cards.storage.write')}>
          <Throughput sample={sampleOf(STORAGE_IO_WRITE_BYTES_KEY)} />
        </Row>
        <Row label={t('cards.storage.readIops')}>
          <Iops sample={sampleOf(STORAGE_IO_READ_IOPS_KEY)} />
        </Row>
        <Row label={t('cards.storage.writeIops')}>
          <Iops sample={sampleOf(STORAGE_IO_WRITE_IOPS_KEY)} />
        </Row>
        <Row label={t('cards.storage.readLatency')}>
          <Latency sample={sampleOf(STORAGE_IO_READ_LATENCY_KEY)} />
        </Row>
        <Row label={t('cards.storage.writeLatency')}>
          <Latency sample={sampleOf(STORAGE_IO_WRITE_LATENCY_KEY)} />
        </Row>
      </dl>

      {telemetry.healthAvailable ? (
        <dl className="kv kv--compact">
          <Row label={t('cards.storage.temperature')}>
            <Temperature sample={sampleOf(STORAGE_HEALTH_TEMPERATURE_KEY)} />
          </Row>
          <Row label={t('cards.storage.usedEndurance')}>
            <Endurance sample={sampleOf(STORAGE_HEALTH_PERCENTAGE_USED_KEY)} />
          </Row>
          <Row label={t('cards.storage.availableSpare')}>
            <Percent sample={sampleOf(STORAGE_HEALTH_AVAILABLE_SPARE_KEY)} />
          </Row>
          <Row label={t('cards.storage.powerOnHours')}>
            <Hours sample={sampleOf(STORAGE_HEALTH_POWER_ON_HOURS_KEY)} />
          </Row>
          <Row label={t('cards.storage.unsafeShutdowns')}>
            <Count sample={sampleOf(STORAGE_HEALTH_UNSAFE_SHUTDOWNS_KEY)} />
          </Row>
          <Row label={t('cards.storage.mediaErrors')}>
            <Count sample={sampleOf(STORAGE_HEALTH_MEDIA_ERRORS_KEY)} />
          </Row>
        </dl>
      ) : (
        <HealthUnavailableNotice reason={telemetry.healthReason} />
      )}

      {volumes.length > 0 && (
        <>
          <h4 className="storage-grid__subtitle">
            {t('metrics.catalog.storage.volume.count.name')}
          </h4>
          <ul className="storage-volumes">
            {volumes.map((volume) => (
              <VolumeEntry key={volume.sourceId} volume={volume} samples={samples} />
            ))}
          </ul>
        </>
      )}
    </li>
  );
}

/** One filesystem: its name, how full it is, and a bar. */
function VolumeEntry({
  volume,
  samples,
}: {
  readonly volume: StorageVolume;
  readonly samples: ReadonlyMap<string, MetricSample>;
}) {
  const sampleOf = (key: string): MetricSample | undefined =>
    samples.get(metricRefId(storageMetric(volume.sourceId, key)));

  const numberOf = (key: string): number | null => {
    const sample = sampleOf(key);
    if (!sample || sample.value === null || sample.value.type !== 'number') return null;
    return sample.value.value;
  };

  const total = numberOf(STORAGE_VOLUME_CAPACITY_TOTAL_KEY);
  const used = numberOf(STORAGE_VOLUME_CAPACITY_USED_KEY);
  const available = numberOf(STORAGE_VOLUME_CAPACITY_AVAILABLE_KEY);
  const percent = numberOf(STORAGE_VOLUME_USAGE_PERCENT_KEY);

  return (
    <li className="storage-volumes__item">
      <div className="storage-volumes__header">
        <span className="storage-volumes__name" title={volume.sourceId}>
          {volume.label}
        </span>
        <span className="storage-volumes__usage">
          {percent !== null ? (
            formatPercent(percent, 0)
          ) : (
            <Unavailable sample={sampleOf(STORAGE_VOLUME_USAGE_PERCENT_KEY)} />
          )}
        </span>
      </div>

      <p className="storage-volumes__size">
        {used !== null && total !== null ? (
          `${formatBytesScaledTo(used, total)} / ${formatBytes(total)}`
        ) : total !== null ? (
          <>
            <Unavailable sample={sampleOf(STORAGE_VOLUME_CAPACITY_USED_KEY)} />
            {` / ${formatBytes(total)}`}
          </>
        ) : (
          <Unavailable sample={sampleOf(STORAGE_VOLUME_CAPACITY_TOTAL_KEY)} />
        )}
        {available !== null && (
          <span className="storage-volumes__available">
            {' '}
            · {t('cards.storage.free', { value: formatBytes(available) })}
          </span>
        )}
      </p>

      {percent !== null && total !== null && total > 0 && (
        <div
          className="usage-bar"
          role="img"
          aria-label={t('cards.storage.full', {
            name: volume.label,
            value: formatPercent(percent, 0),
          })}
        >
          <div className="usage-bar__fill" style={{ width: `${Math.min(100, percent)}%` }} />
        </div>
      )}
    </li>
  );
}

/**
 * States, in the card itself, that this device reports no health values.
 *
 * Without it the six health rows simply vanish and the only explanation lives
 * in a tooltip a user has no reason to look for. The wording is deliberately
 * **"health reporting"**, never "disk unavailable" — the disk is detected,
 * named, measured and perfectly healthy as far as anyone knows; what is
 * missing is the controller's own report.
 */
function HealthUnavailableNotice({ reason }: { readonly reason: string | null }) {
  return (
    <p className="storage-grid__notice" role="note">
      <strong>{t('cards.storage.healthUnavailable')}</strong>
      {reason ?? t('cards.storage.noHealthLog')}
    </p>
  );
}

function Row({ label, children }: { readonly label: string; readonly children: React.ReactNode }) {
  return (
    <div className="kv__row">
      <dt>{label}</dt>
      <dd>{children}</dd>
    </div>
  );
}

/**
 * A throughput — or the reason there is none.
 *
 * `0 B/s` and `—` mean different things and are shown differently: the first
 * is a disk that genuinely moved nothing during the interval; the second is a
 * disk nothing measured.
 */
function Throughput({ sample }: { readonly sample: MetricSample | undefined }) {
  if (sample && sample.value !== null && sample.value.type === 'number') {
    return <>{formatThroughput(sample.value.value)}</>;
  }

  return <Unavailable sample={sample} />;
}

/** Completed operations per second — or the reason there is none. */
function Iops({ sample }: { readonly sample: MetricSample | undefined }) {
  if (sample && sample.value !== null && sample.value.type === 'number') {
    return <>{formatIops(sample.value.value)}</>;
  }

  return <Unavailable sample={sample} />;
}

/**
 * A mean service time — or the reason there is none.
 *
 * The one place where `—` is the *normal* answer for a working disk: an
 * interval in which no operation completed has no mean to report, and showing
 * `0 ms` would claim the device answered instantly.
 */
function Latency({ sample }: { readonly sample: MetricSample | undefined }) {
  if (sample && sample.value !== null && sample.value.type === 'number') {
    return <>{formatLatency(sample.value.value)}</>;
  }

  return <Unavailable sample={sample} />;
}

/** A temperature in Celsius — or the reason there is none. */
function Temperature({ sample }: { readonly sample: MetricSample | undefined }) {
  if (sample && sample.value !== null && sample.value.type === 'number') {
    return <>{formatCelsius(sample.value.value)}</>;
  }

  return <Unavailable sample={sample} />;
}

/**
 * The controller's used-endurance estimate.
 *
 * Rendered without clamping, because the specification permits values above
 * 100 once a drive has passed its rated endurance — which is precisely the
 * situation a user needs to see. {@link formatPercent} refuses anything out of
 * its 0–100 range, so the figure is formatted directly here.
 */
function Endurance({ sample }: { readonly sample: MetricSample | undefined }) {
  if (sample && sample.value !== null && sample.value.type === 'number') {
    const value = sample.value.value;
    return (
      <span className={value > 100 ? 'value--exceeded' : undefined}>
        {`${formatFixed(value, 0)} %`}
      </span>
    );
  }

  return <Unavailable sample={sample} />;
}

/** A bounded 0–100 percentage — or the reason there is none. */
function Percent({ sample }: { readonly sample: MetricSample | undefined }) {
  if (sample && sample.value !== null && sample.value.type === 'number') {
    return <>{formatPercent(sample.value.value, 0)}</>;
  }

  return <Unavailable sample={sample} />;
}

/** A duration in whole hours — or the reason there is none. */
function Hours({ sample }: { readonly sample: MetricSample | undefined }) {
  if (sample && sample.value !== null && sample.value.type === 'number') {
    return <>{formatHours(sample.value.value)}</>;
  }

  return <Unavailable sample={sample} />;
}

/**
 * A plain count — or the reason there is none.
 *
 * Zero is the answer a user hopes for on media errors, so it is shown as the
 * measurement it is.
 */
function Count({ sample }: { readonly sample: MetricSample | undefined }) {
  if (sample && sample.value !== null && sample.value.type === 'number') {
    return <>{formatCount(sample.value.value)}</>;
  }

  return <Unavailable sample={sample} />;
}

/**
 * Explains an absent value instead of showing a zero.
 *
 * The distinction the backend took care to make — an unsupported interface, a
 * permission problem, a missing baseline, an interval with no completed
 * operation — survives into the tooltip, while the cell itself stays a plain
 * dash.
 */
function Unavailable({ sample }: { readonly sample: MetricSample | undefined }) {
  const reason = sample
    ? describeAvailability(sample.availability)
    : t('cards.notReportedByBackend');

  return (
    <span className="value--unavailable" title={reason}>
      —
    </span>
  );
}
