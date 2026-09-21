import { useState } from 'react';
import type { MetricSample } from '@/types/metrics';
import {
  NETWORK_INTERFACE_COUNT,
  NETWORK_INTERFACE_UP_COUNT,
  NETWORK_LINK_RECEIVE_SPEED_KEY,
  NETWORK_LINK_TRANSMIT_SPEED_KEY,
  NETWORK_MTU_KEY,
  NETWORK_PER_WIFI_KEYS,
  NETWORK_RECEIVE_BYTES_KEY,
  NETWORK_RECEIVE_DROPPED_KEY,
  NETWORK_RECEIVE_ERRORS_KEY,
  NETWORK_RECEIVE_PACKETS_KEY,
  NETWORK_TRANSMIT_BYTES_KEY,
  NETWORK_TRANSMIT_DROPPED_KEY,
  NETWORK_TRANSMIT_ERRORS_KEY,
  NETWORK_TRANSMIT_PACKETS_KEY,
  NETWORK_WIFI_LINK_RECEIVE_RATE_KEY,
  NETWORK_WIFI_LINK_TRANSMIT_RATE_KEY,
  NETWORK_WIFI_SIGNAL_QUALITY_KEY,
  NETWORK_WIFI_SIGNAL_RSSI_KEY,
  metricRefId,
} from '@/types/wellknown';
import { useNetworkDetails } from '@/hooks/useNetworkDetails';
import type { NetworkInterface } from '@/utils/network';
import {
  explainMissing,
  networkMetric,
  networkTelemetryState,
  orderInterfaces,
  partitionInterfaces,
} from '@/utils/network';
import { describeAvailability } from '@/utils/metrics';
import {
  formatBitsPerSecond,
  formatBytes,
  formatDbm,
  formatPacketRate,
  formatPercent,
  formatSampleTime,
  formatThroughput,
} from '@/utils/units';

/**
 * Real network inventory, traffic and Wi-Fi quality, sampled on demand.
 *
 * Every number comes from the backend. Nothing is fabricated: an adapter with
 * no link is shown, named and identified, with `—` and a reason on each metric
 * it cannot provide — never `0 B/s` before a baseline exists, which a user
 * would read as "measured, and idle".
 *
 * The interface list is **discovered from the catalog**, so this component
 * contains no list of adapters and works unchanged from zero interfaces to
 * thirty.
 *
 * # Download is receive, Upload is transmit
 *
 * From the machine's point of view. Getting it backwards produces a monitor
 * that is confidently wrong, so the mapping lives in one place — here — and
 * the backend's own descriptions state it too.
 */
export function NetworkDetailsCard() {
  const { status, interfaces, samples, message, refreshing, refresh } = useNetworkDetails();
  const [showAll, setShowAll] = useState(false);

  const countOf = (id: string, fallback: number): number => {
    const sample = samples.get(id);
    return sample?.value?.type === 'number' ? sample.value.value : fallback;
  };

  const interfaceCount = countOf(metricRefId(NETWORK_INTERFACE_COUNT), interfaces.length);
  const connectedCount = countOf(metricRefId(NETWORK_INTERFACE_UP_COUNT), 0);

  // An interface counts as connected when it reports a link speed or any
  // traffic — both of which require the link to be up.
  const isConnected = (sourceId: string): boolean => {
    const state = networkTelemetryState(samples, sourceId);
    return state.linkSpeedAvailable || state.trafficAvailable;
  };

  const ordered = orderInterfaces(interfaces, isConnected);
  const { primary, other } = partitionInterfaces(ordered);
  const timestamp = samples.values().next().value?.timestamp;

  return (
    <div className="card" aria-label="Network details">
      <h2 className="card__title">Network details</h2>

      {status === 'loading' && <p className="card__muted">Discovering network interfaces…</p>}

      {status === 'error' && (
        <p className="card__muted" title={message}>
          Backend unavailable. Run PULSE with <code>pnpm app:dev</code> to reach the Rust layer.
        </p>
      )}

      {status === 'ready' && (
        <>
          <dl className="kv">
            <div className="kv__row">
              <dt>Interfaces</dt>
              <dd>{interfaceCount}</dd>
            </div>
            <div className="kv__row">
              <dt>Connected</dt>
              <dd>{connectedCount}</dd>
            </div>
          </dl>

          {interfaces.length === 0 ? (
            <p className="card__note">
              No network interface was inventoried. The loopback interface is deliberately excluded:
              it always exists and only ever carries traffic that never left this machine.
            </p>
          ) : (
            <ul className="network-grid" aria-label="Network interfaces">
              {primary.map((entry) => (
                <NetworkEntry key={entry.sourceId} entry={entry} samples={samples} />
              ))}
            </ul>
          )}

          {other.length > 0 && (
            <section className="network-other" aria-label="Virtual interfaces">
              <div className="network-other__header">
                <h3 className="network-other__title">Virtual interfaces ({other.length})</h3>
                <button
                  type="button"
                  className="button button--quiet"
                  onClick={() => setShowAll((current) => !current)}
                  aria-expanded={showAll}
                >
                  {showAll ? 'Hide' : 'Show all'}
                </button>
              </div>
              <p className="card__muted">
                Bridges, container links and other software interfaces. Collapsed so a machine
                running containers does not bury its real adapters — never removed.
              </p>

              {showAll && (
                <ul className="network-grid" aria-label="Virtual interface list">
                  {other.map((entry) => (
                    <NetworkEntry key={entry.sourceId} entry={entry} samples={samples} />
                  ))}
                </ul>
              )}
            </section>
          )}

          <div className="card__footer">
            <span className="card__muted">
              {typeof timestamp === 'number' ? `Updated ${formatSampleTime(timestamp)}` : ''}
            </span>
            <button
              type="button"
              className="button"
              onClick={refresh}
              disabled={refreshing}
              aria-label="Refresh network details"
            >
              {refreshing ? 'Refreshing…' : 'Refresh'}
            </button>
          </div>

          <p className="card__note">
            Download is what arrives at this machine and Upload is what leaves it. Traffic is
            measured <em>between</em> two samples, so it appears only after a refresh. Link speeds
            are the capacity the driver negotiated, in bits per second — not how much is flowing,
            which is in bytes. Dropped frames are a local counter and are <strong>not</strong>{' '}
            Internet packet loss, which PULSE does not measure.
          </p>
        </>
      )}
    </div>
  );
}

/** One interface: its name, its state, its traffic, and its Wi-Fi link. */
function NetworkEntry({
  entry,
  samples,
}: {
  readonly entry: NetworkInterface;
  readonly samples: ReadonlyMap<string, MetricSample>;
}) {
  const sampleOf = (key: string): MetricSample | undefined =>
    samples.get(metricRefId(networkMetric(entry.sourceId, key)));

  const numberOf = (key: string): number | null => {
    const sample = sampleOf(key);
    if (!sample || sample.value === null || sample.value.type !== 'number') return null;
    return sample.value.value;
  };

  const telemetry = networkTelemetryState(samples, entry.sourceId);
  const mtu = numberOf(NETWORK_MTU_KEY);
  const connected = telemetry.linkSpeedAvailable || telemetry.trafficAvailable;

  const wifiReason = entry.wireless
    ? explainMissing(samples, entry.sourceId, NETWORK_PER_WIFI_KEYS)
    : null;

  return (
    <li className="network-grid__item">
      <h4 className="network-grid__name" title={entry.sourceId}>
        {entry.label}
      </h4>
      <p className="network-grid__meta">
        {entry.kind}
        {' · '}
        <span className={connected ? 'network-grid__up' : 'network-grid__down'}>
          {connected ? 'Connected' : 'Disconnected'}
        </span>
        {mtu !== null && <span className="network-grid__mtu"> · MTU {formatBytes(mtu, 0)}</span>}
      </p>

      {telemetry.awaitingBaseline && (
        <p className="network-grid__notice" role="note">
          <strong>Waiting for another sample</strong>
          Traffic is the difference between two readings. Refresh to measure it.
        </p>
      )}

      <dl className="kv kv--compact">
        <Row label="Download">
          <Throughput sample={sampleOf(NETWORK_RECEIVE_BYTES_KEY)} />
        </Row>
        <Row label="Upload">
          <Throughput sample={sampleOf(NETWORK_TRANSMIT_BYTES_KEY)} />
        </Row>
        <Row label="RX packets">
          <PacketRate sample={sampleOf(NETWORK_RECEIVE_PACKETS_KEY)} />
        </Row>
        <Row label="TX packets">
          <PacketRate sample={sampleOf(NETWORK_TRANSMIT_PACKETS_KEY)} />
        </Row>
        <Row label="Errors">
          <ErrorPair
            receive={sampleOf(NETWORK_RECEIVE_ERRORS_KEY)}
            transmit={sampleOf(NETWORK_TRANSMIT_ERRORS_KEY)}
          />
        </Row>
        <Row label="Dropped">
          <ErrorPair
            receive={sampleOf(NETWORK_RECEIVE_DROPPED_KEY)}
            transmit={sampleOf(NETWORK_TRANSMIT_DROPPED_KEY)}
          />
        </Row>
        <Row label="Link RX">
          <LinkSpeed sample={sampleOf(NETWORK_LINK_RECEIVE_SPEED_KEY)} />
        </Row>
        <Row label="Link TX">
          <LinkSpeed sample={sampleOf(NETWORK_LINK_TRANSMIT_SPEED_KEY)} />
        </Row>
      </dl>

      {entry.wireless &&
        (wifiReason === null ? (
          <dl className="kv kv--compact">
            <Row label="Signal">
              <Dbm sample={sampleOf(NETWORK_WIFI_SIGNAL_RSSI_KEY)} />
            </Row>
            <Row label="Quality">
              <Quality sample={sampleOf(NETWORK_WIFI_SIGNAL_QUALITY_KEY)} />
            </Row>
            <Row label="Wi-Fi RX">
              <LinkSpeed sample={sampleOf(NETWORK_WIFI_LINK_RECEIVE_RATE_KEY)} />
            </Row>
            <Row label="Wi-Fi TX">
              <LinkSpeed sample={sampleOf(NETWORK_WIFI_LINK_TRANSMIT_RATE_KEY)} />
            </Row>
          </dl>
        ) : (
          <p className="network-grid__notice" role="note">
            <strong>Wi-Fi link details unavailable</strong>
            {wifiReason}
          </p>
        ))}
    </li>
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
 * A throughput in bytes per second — or the reason there is none.
 *
 * `0 B/s` and `—` mean different things and are shown differently: the first
 * is an interface that genuinely moved nothing during the interval; the second
 * is one nothing measured.
 */
function Throughput({ sample }: { readonly sample: MetricSample | undefined }) {
  if (sample && sample.value !== null && sample.value.type === 'number') {
    return <>{formatThroughput(sample.value.value)}</>;
  }

  return <Unavailable sample={sample} />;
}

/** A packet rate — or the reason there is none. */
function PacketRate({ sample }: { readonly sample: MetricSample | undefined }) {
  if (sample && sample.value !== null && sample.value.type === 'number') {
    return <>{formatPacketRate(sample.value.value)}</>;
  }

  return <Unavailable sample={sample} />;
}

/**
 * The receive and transmit halves of an error or drop counter, together.
 *
 * Paired because a user reads them as one question — "is anything going wrong
 * on this interface" — and because two rows of `0 /s` each would take four
 * lines to say nothing.
 */
function ErrorPair({
  receive,
  transmit,
}: {
  readonly receive: MetricSample | undefined;
  readonly transmit: MetricSample | undefined;
}) {
  const value = (sample: MetricSample | undefined): number | null =>
    sample && sample.value !== null && sample.value.type === 'number' ? sample.value.value : null;

  const rx = value(receive);
  const tx = value(transmit);

  if (rx === null && tx === null) return <Unavailable sample={receive} />;

  const format = (rate: number | null) => (rate === null ? '—' : formatPacketRate(rate));
  const clean = rx === 0 && tx === 0;

  return (
    <span className={clean ? undefined : 'value--attention'}>
      {`${format(rx)} RX · ${format(tx)} TX`}
    </span>
  );
}

/**
 * A link capacity in bits per second — or the reason there is none.
 *
 * Deliberately a different unit from the throughput rows above it: a 1 Gbit/s
 * link carrying 12 MiB/s is two numbers in two units, and rendering both in
 * the same one is a factor-of-eight error that looks plausible.
 */
function LinkSpeed({ sample }: { readonly sample: MetricSample | undefined }) {
  if (sample && sample.value !== null && sample.value.type === 'number') {
    return <>{formatBitsPerSecond(sample.value.value)}</>;
  }

  return <Unavailable sample={sample} />;
}

/** A signal strength in dBm — or the reason there is none. */
function Dbm({ sample }: { readonly sample: MetricSample | undefined }) {
  if (sample && sample.value !== null && sample.value.type === 'number') {
    return <>{formatDbm(sample.value.value)}</>;
  }

  return <Unavailable sample={sample} />;
}

/**
 * A platform-provided link quality — or the reason there is none.
 *
 * Fedora reports dBm and no quality percentage, so this row shows `—` with the
 * backend's explanation there. It is never computed from the signal strength.
 */
function Quality({ sample }: { readonly sample: MetricSample | undefined }) {
  if (sample && sample.value !== null && sample.value.type === 'number') {
    return <>{formatPercent(sample.value.value, 0)}</>;
  }

  return <Unavailable sample={sample} />;
}

/**
 * Explains an absent value instead of showing a zero.
 *
 * The distinction the backend took care to make — a missing baseline, an
 * unplugged cable, a platform that reports no quality figure — survives into
 * the tooltip, while the cell itself stays a plain dash.
 */
function Unavailable({ sample }: { readonly sample: MetricSample | undefined }) {
  const reason = sample ? describeAvailability(sample.availability) : 'Not reported by the backend';

  return (
    <span className="value--unavailable" title={reason}>
      —
    </span>
  );
}
