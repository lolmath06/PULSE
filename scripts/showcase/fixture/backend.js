/*
 * PULSE showcase fixture backend.
 *
 * SHOWCASE TOOLING ONLY. This file is never bundled into PULSE. The capture
 * scripts inject it into a plain Chromium page *before* the real, unmodified
 * PULSE frontend loads, as `window.__TAURI_INTERNALS__` — the same seam the
 * Tauri webview provides. Every component, layout and style on screen is the
 * application's own; only the answers to its backend commands come from here.
 *
 * The hardware is fictional and the readings are deterministic functions of
 * time (no randomness), so captures are reproducible and reveal nothing about
 * the machine they were recorded on. The metric catalog itself is produced by
 * PULSE's real Rust declarations — see ../catalog.
 *
 * Expects `window.__PULSE_SHOWCASE__ = { catalog, sourceRefs, config? }` to be
 * defined first (the capture scripts do this).
 */
(() => {
  'use strict';

  const fixture = window.__PULSE_SHOWCASE__;
  if (!fixture) throw new Error('showcase fixture missing');

  const AVAILABLE = { status: 'available' };
  const GIB = 1024 ** 3;

  // --- deterministic signal helpers ------------------------------------------

  function hash(text) {
    let h = 2166136261;
    for (let i = 0; i < text.length; i += 1) {
      h ^= text.charCodeAt(i);
      h = Math.imul(h, 16777619);
    }
    // murmur3 finalizer: neighbouring inputs must not give neighbouring outputs.
    h ^= h >>> 16;
    h = Math.imul(h, 0x85ebca6b);
    h ^= h >>> 13;
    h = Math.imul(h, 0xc2b2ae35);
    h ^= h >>> 16;
    return (h >>> 0) / 4294967295;
  }

  /** Smoothly interpolated lattice noise in [-1, 1]. */
  function lattice(seed, x) {
    const i = Math.floor(x);
    const f = x - i;
    const u = f * f * (3 - 2 * f);
    const a = hash(seed + ':' + i) * 2 - 1;
    const b = hash(seed + ':' + (i + 1)) * 2 - 1;
    return a + (b - a) * u;
  }

  /** Organic pseudo-noise in about [-1, 1], deterministic in (seed, seconds). */
  function wave(seed, s) {
    return (
      0.5 * lattice(seed + 'a', s / 41) +
      0.3 * lattice(seed + 'b', s / 9.7) +
      0.15 * lattice(seed + 'c', s / 2.3) +
      0.08 * lattice(seed + 'd', s)
    );
  }

  /**
   * Irregular activity episodes in [0, 1] — a build, a download, a shader
   * compile: each window of `period` seconds may hold one episode of varying
   * length and strength, with soft edges.
   */
  function burst(seed, s, period = 90, width = 14, chance = 0.55) {
    let level = 0;
    const k0 = Math.floor(s / period);
    for (let k = k0 - 2; k <= k0; k += 1) {
      if (hash(seed + 'e' + k) > chance) continue;
      const w = width * (0.5 + hash(seed + 'w' + k));
      const start = k * period + hash(seed + 'o' + k) * Math.max(0, period - w * 0.4);
      const x = (s - start) / w;
      if (x <= 0 || x >= 1) continue;
      const edge = Math.min(1, x / 0.15, (1 - x) / 0.2);
      const shape = edge * edge * (3 - 2 * edge);
      // Textured, not flat: real workloads breathe while they run.
      const texture = 0.78 + 0.22 * lattice(seed + 't' + k, s / 1.7);
      level = Math.max(level, shape * texture * (0.5 + 0.5 * hash(seed + 'a' + k)));
    }
    return level;
  }

  /** Day-shaped activity for long history ranges, 0.35–1. */
  function activity(ms) {
    const hour = new Date(ms).getUTCHours() + new Date(ms).getUTCMinutes() / 60;
    const day = Math.exp(-((hour - 15) ** 2) / 18);
    return 0.35 + 0.65 * day;
  }

  const clamp = (v, lo, hi) => Math.min(hi, Math.max(lo, v));

  // --- the fictional machine --------------------------------------------------

  const LOGICAL = 16;
  const MEMORY_TOTAL = 32 * GIB - 812 * 1024 ** 2;
  const VRAM_TOTAL = 12 * GIB;

  function cpuLogical(i, ms) {
    const s = ms / 1000;
    const lane = 'cpu' + i;
    const base = 14 + (i % 2 === 0 ? 10 : 4) + (i < 4 ? 8 : 0);
    const shared = 9 * wave('cpu-shared', s) + 5 * lattice('cpu-flicker', s / 1.3);
    const v =
      (base +
        shared +
        9 * wave(lane, s) +
        44 * burst('build', s, 170, 75, 0.6) * (0.6 + 0.4 * hash(lane))) *
      activity(ms);
    return clamp(v, 1, 100);
  }

  /** Thermal inertia: heat follows load over the previous ~25 s. */
  function thermal(load, ms) {
    let sum = 0;
    for (let k = 0; k < 6; k += 1) sum += load(ms - k * 5_000) * (6 - k);
    return sum / 21;
  }

  function cpuTotal(ms) {
    let sum = 0;
    for (let i = 0; i < LOGICAL; i += 1) sum += cpuLogical(i, ms);
    return sum / LOGICAL;
  }

  const volumeTotals = {};
  const gpuHeat = (sourceId, ms) => thermal((t) => numberFor('gpu.usage.core', sourceId, t), ms);

  function numberFor(key, sourceId, ms) {
    const s = ms / 1000;
    const seed = key + '@' + sourceId;
    const n = wave(seed, s);
    const act = activity(ms);
    const isData = sourceId.endsWith('0002') || sourceId.endsWith('0002-p1');
    const isWifi = sourceId.endsWith('e002');

    switch (key) {
      case 'cpu.usage.total':
        return cpuTotal(ms);
      case 'cpu.usage.logical':
        return cpuLogical(Number(sourceId.split('-').pop()), ms);
      case 'cpu.frequency.current': {
        const i = Number(sourceId.split('-').pop());
        return Math.round(2.9e9 + cpuLogical(i, ms) * 2.1e7 + 1.2e8 * n);
      }
      case 'cpu.frequency.max':
        return 5.4e9;
      case 'cpu.count.logical':
        return LOGICAL;
      case 'cpu.count.physical':
        return 8;
      case 'cpu.count.package':
        return 1;
      case 'cpu.temperature.package':
        return 40 + thermal(cpuTotal, ms) * 0.45 + 1.2 * n;

      case 'memory.total':
        return MEMORY_TOTAL;
      case 'memory.usage.percent':
        return 44 + 6 * act + 2.5 * n;
      case 'memory.used':
        return (MEMORY_TOTAL * numberFor('memory.usage.percent', sourceId, ms)) / 100;
      case 'memory.available':
        return MEMORY_TOTAL - numberFor('memory.used', sourceId, ms);

      case 'gpu.count':
        return 1;
      case 'gpu.usage.core':
        return clamp(
          (46 + 18 * n + 6 * lattice(seed + 'f', s / 1.1) + 26 * burst('render', s, 150, 70, 0.6)) *
            act,
          2,
          99,
        );
      case 'gpu.memory.total':
        return VRAM_TOTAL;
      case 'gpu.memory.usage.percent':
        return 52 + 6 * act + 3 * n;
      case 'gpu.memory.used':
        return (VRAM_TOTAL * numberFor('gpu.memory.usage.percent', sourceId, ms)) / 100;
      case 'gpu.memory.free':
        return VRAM_TOTAL - numberFor('gpu.memory.used', sourceId, ms);
      case 'gpu.frequency.core':
        return Math.round(1.9e9 + numberFor('gpu.usage.core', sourceId, ms) * 8e6);
      case 'gpu.frequency.memory':
        return 10.501e9;
      case 'gpu.temperature.core':
        return 42 + gpuHeat(sourceId, ms) * 0.32 + n;
      case 'gpu.temperature.hotspot':
        return 52 + gpuHeat(sourceId, ms) * 0.36 + n;
      case 'gpu.temperature.memory':
        return 50 + gpuHeat(sourceId, ms) * 0.24 + n;
      case 'gpu.fan.speed':
        return Math.round(900 + numberFor('gpu.usage.core', sourceId, ms) * 12);

      case 'storage.device.count':
        return 2;
      case 'storage.volume.count':
        return 2;
      case 'storage.capacity.total':
        return isData ? 1_000_204_886_016 : 2_000_398_934_016;
      case 'storage.io.read.bytes_per_second':
        return Math.max(
          0,
          (isData ? 4e6 : 18e6) * (0.4 + 0.3 * n) * act + 380e6 * burst(seed, s, 130, 16, 0.5),
        );
      case 'storage.io.write.bytes_per_second':
        return Math.max(
          0,
          (isData ? 2e6 : 9e6) * (0.4 + 0.3 * n) * act + 140e6 * burst(seed, s, 150, 20, 0.45),
        );
      case 'storage.io.read.iops':
        return numberFor('storage.io.read.bytes_per_second', sourceId, ms) / 48_000;
      case 'storage.io.write.iops':
        return numberFor('storage.io.write.bytes_per_second', sourceId, ms) / 64_000;
      case 'storage.io.read.latency':
        return (isData ? 0.35 : 0.09) + 0.03 * n;
      case 'storage.io.write.latency':
        return (isData ? 0.6 : 0.16) + 0.04 * n;
      case 'storage.health.temperature':
        return (isData ? 34 : 41) + 2 * act + 0.8 * n;
      case 'storage.health.percentage_used':
        return 3;
      case 'storage.health.available_spare':
        return 100;
      case 'storage.health.power_on_hours':
        return isData ? 9120 : 2184;
      case 'storage.health.unsafe_shutdowns':
        return 11;
      case 'storage.health.media_errors':
        return 0;

      case 'storage.volume.capacity.total':
        return (volumeTotals[sourceId] ??= isData ? 983_349_346_304 : 1_998_694_793_216);
      case 'storage.volume.usage.percent':
        return isData ? 63.4 : 41.7 + 0.02 * n;
      case 'storage.volume.capacity.used':
        return (
          (numberFor('storage.volume.capacity.total', sourceId, ms) *
            numberFor('storage.volume.usage.percent', sourceId, ms)) /
          100
        );
      case 'storage.volume.capacity.available':
        return (
          numberFor('storage.volume.capacity.total', sourceId, ms) -
          numberFor('storage.volume.capacity.used', sourceId, ms)
        );

      case 'network.interface.count':
        return 2;
      case 'network.interface.up_count':
        return 2;
      case 'network.link.receive_speed':
      case 'network.link.transmit_speed':
        return isWifi ? 1.2e9 : 2.5e9;
      case 'network.mtu':
        return 1500;
      case 'network.receive.bytes_per_second':
        return Math.max(
          0,
          isWifi
            ? 60e3 + 40e3 * n
            : (1.4e6 + 9e5 * n) * act + 46e6 * burst('download', s, 220, 60, 0.5),
        );
      case 'network.transmit.bytes_per_second':
        return Math.max(
          0,
          isWifi
            ? 12e3 + 8e3 * n
            : (3.2e5 + 2e5 * n) * act + 5e6 * burst('upload', s, 180, 30, 0.4),
        );
      case 'network.receive.packets_per_second':
        return numberFor('network.receive.bytes_per_second', sourceId, ms) / 1350;
      case 'network.transmit.packets_per_second':
        return numberFor('network.transmit.bytes_per_second', sourceId, ms) / 900;
      case 'network.receive.errors_per_second':
      case 'network.transmit.errors_per_second':
      case 'network.receive.dropped_per_second':
      case 'network.transmit.dropped_per_second':
        return 0;
      case 'network.wifi.link.receive_rate':
        return 1.201e9;
      case 'network.wifi.link.transmit_rate':
        return 864.8e6;
      case 'network.wifi.signal.quality':
        return 78 + 3 * n;
      case 'network.wifi.signal.rssi':
        return Math.round(-52 + 2 * n);

      case 'process.count.total':
        return Math.round(412 + 9 * n);
      case 'process.count.running':
        return Math.max(1, Math.round(3 + 2 * n));
      case 'process.thread.count.total':
        return Math.round(1873 + 40 * n);
      default:
        return null;
    }
  }

  // --- catalog ----------------------------------------------------------------

  const catalog = fixture.catalog;
  const definitions = new Map(catalog.map((d) => [d.metric.key + '@' + d.metric.sourceId, d]));

  function sample(ref, ms) {
    const definition = definitions.get(ref.key + '@' + ref.sourceId);
    if (!definition) {
      return {
        metric: ref,
        timestamp: ms,
        value: null,
        availability: { status: 'notRegistered', reason: 'not in the showcase catalog' },
      };
    }
    if (definition.availability.status !== 'available') {
      return { metric: ref, timestamp: ms, value: null, availability: definition.availability };
    }
    const v = numberFor(ref.key, ref.sourceId, ms);
    return {
      metric: ref,
      timestamp: ms,
      value: v === null ? null : { type: 'number', value: v },
      availability:
        v === null ? { status: 'temporarilyUnavailable', reason: 'no data' } : AVAILABLE,
    };
  }

  function engineStatus() {
    const providers = new Map();
    for (const d of catalog) {
      const p = providers.get(d.providerId) ?? {
        id: d.providerId,
        metricCount: 0,
        availableMetricCount: 0,
      };
      p.metricCount += 1;
      if (d.availability.status === 'available') p.availableMetricCount += 1;
      providers.set(d.providerId, p);
    }
    const list = [...providers.values()];
    return {
      schemaVersion: 1,
      state: 'ready',
      providerCount: list.length,
      metricCount: catalog.length,
      availableMetricCount: list.reduce((n, p) => n + p.availableMetricCount, 0),
      providers: list,
    };
  }

  // --- history ----------------------------------------------------------------

  const RANGES = {
    '15m': [15 * 60_000, 5_000],
    '1h': [3_600_000, 5_000],
    '6h': [6 * 3_600_000, 30_000],
    '24h': [24 * 3_600_000, 120_000],
    '7d': [7 * 24 * 3_600_000, 900_000],
  };

  function history(metrics, range) {
    const [span, bucket] = RANGES[range] ?? RANGES['15m'];
    const raw = bucket <= 5_000;
    const toMs = Math.floor(Date.now() / bucket) * bucket;
    const fromMs = toMs - span;
    const series = metrics.map((ref) => {
      const points = [];
      for (let t = fromMs; t <= toMs; t += bucket) {
        const s = sample(ref, t);
        if (!s.value) continue;
        if (raw) {
          points.push({ t, v: s.value.value });
        } else {
          let min = Infinity;
          let max = -Infinity;
          let sum = 0;
          const n = 6;
          for (let k = 0; k < n; k += 1) {
            const v = numberFor(ref.key, ref.sourceId, t + (k * bucket) / n);
            min = Math.min(min, v);
            max = Math.max(max, v);
            sum += v;
          }
          points.push({ t, v: sum / n, min, max, n: bucket / 5_000 });
        }
      }
      const last = points[points.length - 1];
      return { metric: ref, points, latest: last ? { t: last.t, v: last.v } : null };
    });
    return {
      status: 'ok',
      range,
      fromMs,
      toMs,
      bucketMs: bucket,
      raw,
      gapThresholdMs: bucket * 3,
      series,
    };
  }

  const SESSION_START = Date.now();

  function historyStatus(includeDatabase) {
    const batches = Math.floor((Date.now() - SESSION_START) / 5_000) + 1;
    return {
      state: 'recording',
      databasePath: '~/.local/share/dev.pulse.app/history.sqlite3',
      schemaVersion: 1,
      cadenceMs: 5_000,
      historizedMetricCount: catalog.filter((d) => d.unit !== 'none').length,
      batchesThisSession: batches,
      lastBatch: { batchId: 48_211 + batches, timestampMs: Date.now(), rowCount: 112 },
      lastError: null,
      timings: {
        ticks: batches,
        sampleMedianUs: 1_840,
        sampleMaxUs: 4_920,
        insertMedianUs: 610,
        insertMaxUs: 2_310,
      },
      lastCompaction: {
        atMs: Date.now() - 22 * 60_000,
        durationMs: 38,
        report: {
          aggregatesWritten: 1_344,
          rawRowsDeleted: 80_640,
          aggregatesDeleted: 0,
          batchesDeleted: 720,
        },
      },
      database: includeDatabase
        ? {
            path: '~/.local/share/dev.pulse.app/history.sqlite3',
            schemaVersion: 1,
            journalMode: 'wal',
            synchronous: 1,
            busyTimeoutMs: 5_000,
            fileBytes: 46_137_344,
            walBytes: 1_236_992,
            pageSize: 4_096,
            pageCount: 11_264,
            freelistCount: 12,
            seriesCount: 112,
            rawRows: 1_935_360,
            aggregateRows: 96_768,
            batchCount: 17_280,
            averageRowsPerBatch: 112,
            firstBatchMs: Date.now() - 7 * 24 * 3_600_000,
            lastBatchMs: Date.now(),
          }
        : null,
    };
  }

  // --- processes ----------------------------------------------------------------

  // A plausible Fedora workstation mid-session. Short OS names only: PULSE
  // never collects command lines.
  const PROCESSES = [
    // name, exe, classification, cpu%, MiB, threads, pid, ppid, read B/s, write B/s, count
    ['systemd', '/usr/lib/systemd/systemd', 'systemProcess', 0.1, 18, 1, 1, null],
    ['systemd-journald', '/usr/lib/systemd/systemd-journald', 'systemProcess', 0.1, 42, 1, 712, 1],
    ['NetworkManager', '/usr/sbin/NetworkManager', 'systemProcess', 0.0, 21, 4, 1084, 1],
    ['pipewire', '/usr/bin/pipewire', 'userApplication', 0.6, 24, 3, 2203, 2110],
    ['wireplumber', '/usr/bin/wireplumber', 'userApplication', 0.1, 31, 6, 2204, 2110],
    ['gnome-shell', '/usr/bin/gnome-shell', 'userApplication', 3.4, 412, 24, 2311, 2110],
    ['Xwayland', '/usr/bin/Xwayland', 'userApplication', 0.4, 96, 9, 2402, 2311],
    ['firefox', '/usr/lib64/firefox/firefox', 'userApplication', 2.1, 610, 92, 3120, 2311],
    ['Isolated Web Co', '/usr/lib64/firefox/firefox', 'userApplication', 1.6, 384, 28, 3188, 3120],
    ['Isolated Web Co', '/usr/lib64/firefox/firefox', 'userApplication', 0.4, 212, 24, 3214, 3120],
    ['Web Content', '/usr/lib64/firefox/firefox', 'userApplication', 0.1, 96, 19, 3260, 3120],
    ['code', '/usr/share/code/code', 'userApplication', 1.2, 288, 34, 4011, 2311],
    ['code', '/usr/share/code/code', 'userApplication', 0.8, 512, 22, 4044, 4011],
    ['rust-analyzer', '/usr/bin/rust-analyzer', 'userApplication', 4.8, 1_640, 18, 4102, 4044],
    ['cargo', '/usr/bin/cargo', 'userApplication', 0.3, 64, 12, 5310, 5122],
    ['rustc', '/usr/bin/rustc', 'userApplication', 11.6, 820, 17, 5341, 5310],
    ['rustc', '/usr/bin/rustc', 'userApplication', 8.9, 690, 17, 5342, 5310],
    ['node', '/usr/bin/node', 'userApplication', 1.9, 248, 11, 5402, 5122],
    ['steam', '/usr/lib/steam/steam', 'userApplication', 0.7, 340, 61, 6120, 2311],
    [
      'steamwebhelper',
      '/usr/lib/steam/steamwebhelper',
      'userApplication',
      0.5,
      290,
      30,
      6188,
      6120,
    ],
    ['ptyxis', '/usr/bin/ptyxis', 'userApplication', 0.2, 88, 9, 5100, 2311],
    ['bash', '/usr/bin/bash', 'userApplication', 0.0, 6, 1, 5122, 5100],
    ['nautilus', '/usr/bin/nautilus', 'userApplication', 0.0, 142, 11, 4710, 2311],
    ['pulse', '/usr/bin/pulse', 'userApplication', 0.9, 128, 21, 6600, 2311],
    [
      'WebKitWebProcess',
      '/usr/libexec/webkit2gtk-4.1/WebKitWebProcess',
      'userApplication',
      0.6,
      176,
      18,
      6612,
      6600,
    ],
    ['gnome-software', '/usr/bin/gnome-software', 'userApplication', 0.0, 164, 9, 2988, 2311],
    ['podman', '/usr/bin/podman', 'userApplication', 0.2, 58, 14, 5520, 5122],
    ['postgres', '/usr/bin/postgres', 'userApplication', 0.4, 46, 1, 5541, 5520],
    ['kworker/u32:3', null, 'kernelThread', 0.1, 0, 1, 980, 2],
    ['kthreadd', null, 'kernelThread', 0.0, 0, 1, 2, null],
  ];

  function field(value) {
    return { value, availability: AVAILABLE };
  }

  function processSnapshot() {
    const ms = Date.now();
    const s = ms / 1000;
    const processes = PROCESSES.map(([name, exe, cls, cpu, mib, threads, pid, ppid]) => {
      const n = wave(name + pid, s);
      const busy = name === 'rustc' ? 1 + burst('build', s, 160, 70) * 2 : 1;
      const cpuPercent = Math.max(0, cpu * busy * (1 + 0.25 * n));
      const kernel = cls === 'kernelThread';
      const rss = mib * 1024 ** 2 * (1 + 0.02 * n);
      return {
        instanceId: `process:${pid}-${40_000 + pid * 7}`,
        pid,
        parentPid: ppid,
        name,
        executablePath: exe
          ? field(exe)
          : { value: null, availability: { status: 'notDetected', reason: 'kernel thread' } },
        state: cpuPercent > 3 ? 'running' : 'sleepingOrWaiting',
        stateAvailability: AVAILABLE,
        classification: cls,
        cpuPercent: field(cpuPercent),
        residentMemoryBytes: field(kernel ? 0 : rss),
        memoryPercent: field(kernel ? 0 : (rss / MEMORY_TOTAL) * 100),
        threadCount: field(threads),
        readBytesPerSecond: field(name === 'rustc' ? 2.4e6 * busy : cpuPercent * 9_000),
        writeBytesPerSecond: field(name === 'rustc' ? 1.1e6 * busy : cpuPercent * 3_000),
        applicationKey: exe ? 'exe:' + exe : 'name:' + name,
      };
    });

    const apps = new Map();
    for (const p of processes) {
      const name = p.executablePath.value ? p.executablePath.value.split('/').pop() : p.name;
      const app = apps.get(p.applicationKey) ?? {
        key: p.applicationKey,
        displayName: name,
        identity: p.executablePath.value ? 'executable' : 'name',
        classification: p.classification,
        processCount: 0,
        threads: 0,
        cpu: 0,
        rss: 0,
        read: 0,
        write: 0,
      };
      app.processCount += 1;
      app.threads += p.threadCount.value;
      app.cpu += p.cpuPercent.value;
      app.rss += p.residentMemoryBytes.value;
      app.read += p.readBytesPerSecond.value;
      app.write += p.writeBytesPerSecond.value;
      apps.set(p.applicationKey, app);
    }
    const applications = [...apps.values()].map((a) => ({
      key: a.key,
      displayName: a.displayName,
      identity: a.identity,
      classification: a.classification,
      processCount: a.processCount,
      threadCount: field(a.threads),
      cpuPercent: field(a.cpu),
      residentMemoryBytes: field(a.rss),
      readBytesPerSecond: field(a.read),
      writeBytesPerSecond: field(a.write),
    }));

    return {
      takenAt: ms,
      durationMs: 6.4,
      counts: {
        total: numberFor('process.count.total', 'process:system', ms),
        running: numberFor('process.count.running', 'process:system', ms),
        threads: numberFor('process.thread.count.total', 'process:system', ms),
      },
      processes,
      applications,
      unsupportedReason: null,
    };
  }

  const OK = {
    status: 'success',
    reason: '',
    affectedCount: null,
    failedCount: null,
    tree: null,
  };
  const allowed = { allowed: true, reason: null };

  function findProcess(instanceId) {
    return processSnapshot().processes.find((p) => p.instanceId === instanceId);
  }

  function processDetails(instanceId) {
    const p = findProcess(instanceId);
    if (!p) {
      return {
        outcome: { ...OK, status: 'processGone', reason: 'The process has exited.' },
        value: null,
      };
    }
    const parent = PROCESSES.find((row) => row[6] === p.parentPid);
    const exe = p.executablePath.value;
    const self = p.name === 'pulse';
    return {
      outcome: OK,
      value: {
        instanceId,
        pid: p.pid,
        parentPid: p.parentPid,
        parentName: parent
          ? field(parent[0])
          : { value: null, availability: { status: 'notDetected', reason: 'no parent' } },
        name: p.name,
        state: p.state,
        stateAvailability: AVAILABLE,
        category: p.classification,
        startedAt: field(SESSION_START - (3 * 3_600_000 + p.pid * 1_000)),
        executable: exe
          ? field({
              path: exe,
              fileName: exe.split('/').pop(),
              sizeBytes: field(1_000_000 + (p.pid % 97) * 314_159),
              modifiedAt: field(SESSION_START - 12 * 24 * 3_600_000),
              replacedOnDisk: false,
            })
          : { value: null, availability: { status: 'notDetected', reason: 'kernel thread' } },
        owner: field({ id: '1000', name: 'demo' }),
        architecture: field('x86_64'),
        priority: field({ kind: 'nice', value: 0 }),
        affinity: field({
          cpus: [...Array(LOGICAL).keys()],
          available: [...Array(LOGICAL).keys()],
          limitation: null,
        }),
        versionInfo: {
          value: null,
          availability: {
            status: 'unsupported',
            reason: 'Version resources exist on Windows only.',
          },
        },
        capabilities: {
          terminate: allowed,
          terminateTree: allowed,
          forceKill: allowed,
          suspend: self ? { allowed: false, reason: 'PULSE will not suspend itself.' } : allowed,
          resume: allowed,
          setPriority: allowed,
          setAffinity: allowed,
          openLocation: allowed,
          computeHash: allowed,
        },
        isSelf: self,
        suspendedByPulse: false,
        priorityKind: 'nice',
        forceKillSupported: true,
      },
    };
  }

  // Which Fedora package owns each executable, with plausible versions.
  const PACKAGES = {
    rustc: ['rust', '1.79.0-1.fc39'],
    cargo: ['cargo', '1.79.0-1.fc39'],
    'rust-analyzer': ['rust-analyzer', '1.79.0-1.fc39'],
    firefox: ['firefox', '128.0-1.fc39'],
    'gnome-shell': ['gnome-shell', '45.6-1.fc39'],
    node: ['nodejs', '20.15.0-1.fc39'],
  };

  function provenance(instanceId) {
    const p = findProcess(instanceId);
    const exe = p?.executablePath.value ?? '';
    const file = exe.split('/').pop() || 'unknown';
    const [name, version] = PACKAGES[file] ?? [file, '1.0-1.fc39'];
    return {
      outcome: OK,
      value: { kind: 'rpmPackage', packages: [{ name, version, arch: 'x86_64' }] },
    };
  }

  // --- desktop ----------------------------------------------------------------

  // Strings below are the backend's own (src-tauri/src/overlay/backend.rs and
  // capabilities.rs) for Wayland with the GNOME bridge active.
  const cap = (status, reason) => ({ status, reason });
  function desktopStatus() {
    return {
      backend: {
        kind: 'gnomeBridge',
        label: 'GNOME native bridge',
        detail:
          'The PULSE extension inside GNOME Shell keeps overlays above other windows (Mutter make_above) and delivers the overlay shortcut through Mutter; PULSE keeps locked overlays click-through. Verified on Fedora 39 / GNOME 45.',
        verification: 'physicallyVerified',
      },
      gnomeBridge: {
        state: 'active',
        summary: 'Active — keeping overlays above and delivering the shortcut',
        guidance: null,
        shellVersion: '45.0',
        runningVersion: 2,
        installedVersion: 2,
        bundledVersion: 2,
        installPath: '~/.local/share/gnome-shell/extensions/pulse-overlay@jamby',
        installedByPulse: true,
        connected: true,
        restartPending: false,
        updateAvailable: false,
        canEnable: false,
        canDisable: true,
        error: null,
      },
      gnomeBridgeSource: null,
      capabilities: {
        displayServer: 'wayland',
        alwaysOnTop: cap(
          'supported',
          'kept above other windows by the PULSE GNOME bridge (Mutter make_above), whichever application has focus — verified on GNOME 45',
        ),
        clickThrough: cap(
          'supported',
          "an empty input region, kept as GTK's own input shape so every compositor configure preserves it — verified on GNOME 45 (Fedora 39)",
        ),
        positioning: cap(
          'unsupported',
          'Wayland clients cannot choose or read their position; drag the overlay in Edit mode and the compositor places it',
        ),
        transparentWindow: cap('supported', 'per-pixel alpha surfaces'),
        globalHotkey: cap(
          'supported',
          'Ctrl+Alt+P is registered with Mutter by the PULSE GNOME bridge and reaches PULSE whichever application has focus — verified on GNOME 45',
        ),
        multiMonitorPositioning: cap(
          'unsupported',
          'the compositor chooses the monitor; PULSE cannot place a window on one',
        ),
        tray: cap(
          'limited',
          'StatusNotifier/AppIndicator: shown by KDE; GNOME needs the AppIndicator extension',
        ),
      },
      hotkey: 'Ctrl+Alt+P',
      hotkeyError: null,
      hotkeyBackend: { kind: 'plugin' },
    };
  }

  // --- UI configuration -------------------------------------------------------

  const CONFIG_KEY = 'pulse-showcase-ui-config';
  function loadDocument() {
    try {
      const saved = sessionStorage.getItem(CONFIG_KEY);
      if (saved) return JSON.parse(saved);
    } catch {
      /* fresh */
    }
    return { version: 1, ...(fixture.config ?? {}) };
  }
  let documentState = loadDocument();
  let revision = 1;

  // --- events -------------------------------------------------------------------

  const callbacks = new Map();
  let nextCallback = 1;
  const listeners = new Map();
  let nextEvent = 1;

  function emit(event, payload) {
    for (const [id, handler] of listeners.get(event) ?? []) {
      callbacks.get(handler)?.({ event, id, payload });
    }
  }

  let liveRefs = [];
  setInterval(() => {
    if (liveRefs.length === 0) return;
    const t = Math.floor(Date.now() / 1000) * 1000;
    emit('live-sample', {
      t,
      values: liveRefs.map((metric) => ({ metric, v: sample(metric, t).value?.value ?? null })),
    });
  }, 1000);
  setInterval(() => {
    emit('history-sample-recorded', {
      batchId: Math.floor(Date.now() / 5000),
      timestampMs: Date.now(),
      rowCount: 112,
    });
  }, 5000);

  // --- commands -------------------------------------------------------------------

  const unknown = new Set();

  function handle(cmd, args = {}) {
    switch (cmd) {
      case 'plugin:event|listen': {
        const id = nextEvent++;
        const list = listeners.get(args.event) ?? new Map();
        list.set(id, args.handler);
        listeners.set(args.event, list);
        return id;
      }
      case 'plugin:event|unlisten':
        listeners.get(args.event)?.delete(args.eventId);
        return null;
      case 'plugin:event|emit':
      case 'plugin:event|emit_to':
        emit(args.event, args.payload);
        return null;
      case 'plugin:window|primary_monitor':
      case 'plugin:window|current_monitor':
        return {
          name: 'Showcase display',
          size: { width: 2560, height: 1440 },
          position: { x: 0, y: 0 },
          workArea: { position: { x: 0, y: 0 }, size: { width: 2560, height: 1440 } },
          scaleFactor: 1,
        };
      case 'plugin:window|available_monitors':
        return [handle('plugin:window|primary_monitor')];

      case 'get_platform_info':
        return {
          platform: 'linux',
          os: 'linux',
          arch: 'x86_64',
          osVersion: 'Fedora Linux 39 (Workstation Edition)',
          displayServer: 'wayland',
          appVersion: '0.1.0-dev',
        };
      case 'get_metrics_engine_status':
        return engineStatus();
      case 'get_metric_catalog':
        return catalog;
      case 'get_source_refs':
        return fixture.sourceRefs;
      case 'sample_metrics': {
        const ms = Date.now();
        return args.metrics.map((ref) => sample(ref, ms));
      }
      case 'set_live_subscription':
        liveRefs = args.metrics;
        return { refused: [] };
      case 'get_live_buffer': {
        const now = Math.floor(Date.now() / 1000) * 1000;
        return args.metrics.map((metric) => ({
          metric,
          points: Array.from({ length: 300 }, (_, i) => {
            const t = now - (300 - i) * 1000;
            return { t, v: sample(metric, t).value?.value ?? null };
          }),
        }));
      }
      case 'get_metric_history':
        return history(args.metrics, args.range);
      case 'get_history_status':
        return historyStatus(args.includeDatabase);

      case 'get_process_snapshot':
        return processSnapshot();
      case 'get_process_details':
        return processDetails(args.instanceId);
      case 'get_process_provenance':
        return provenance(args.instanceId);
      case 'compute_process_sha256':
        return {
          outcome: OK,
          value: {
            status: 'computed',
            sha256: [...Array(64)]
              .map((_, i) => '0123456789abcdef'[Math.floor(hash(args.instanceId + i) * 16)])
              .join(''),
            sizeBytes: 31_457_280,
            reason: null,
          },
        };
      case 'get_process_priority':
        return { outcome: OK, value: { kind: 'nice', value: 0 } };
      case 'get_process_affinity':
        return {
          outcome: OK,
          value: {
            cpus: [...Array(LOGICAL).keys()],
            available: [...Array(LOGICAL).keys()],
            limitation: null,
          },
        };
      case 'suspend_process':
      case 'resume_process':
      case 'terminate_process':
      case 'terminate_process_tree':
      case 'set_process_priority':
      case 'set_process_affinity':
      case 'open_process_location':
      case 'open_web_search':
      case 'open_hash_lookup':
        // The showcase never acts on anything.
        return { ...OK, status: 'unsupported', reason: 'Showcase fixture: no action taken.' };

      case 'get_desktop_status':
      case 'refresh_gnome_bridge':
      case 'set_gnome_bridge_enabled':
        return desktopStatus();
      case 'set_overlay_hotkey':
        return args.shortcut ?? null;
      case 'overlay_action':
      case 'open_main_window':
      case 'open_mini_window':
      case 'quit_app':
        return null;

      case 'get_ui_config':
        return {
          revision,
          document: structuredClone(documentState),
          load: { kind: 'loaded' },
          readOnly: null,
          path: '~/.config/dev.pulse.app/ui-config.json',
        };
      case 'set_ui_config_section':
        documentState = { ...documentState, [args.section]: structuredClone(args.value) };
        revision += 1;
        try {
          sessionStorage.setItem(CONFIG_KEY, JSON.stringify(documentState));
        } catch {
          /* best effort */
        }
        return revision;

      default:
        if (!unknown.has(cmd)) {
          unknown.add(cmd);
          console.warn('[showcase] unhandled command', cmd);
        }
        return null;
    }
  }

  window.__TAURI_INTERNALS__ = {
    metadata: {
      currentWindow: { label: 'main' },
      currentWebview: { windowLabel: 'main', label: 'main' },
    },
    plugins: {},
    transformCallback(callback, once = false) {
      const id = nextCallback++;
      callbacks.set(id, (data) => {
        if (once) callbacks.delete(id);
        return callback?.(data);
      });
      return id;
    },
    unregisterCallback(id) {
      callbacks.delete(id);
    },
    runCallback(id, data) {
      callbacks.get(id)?.(data);
    },
    callbacks,
    convertFileSrc: (path) => path,
    invoke(cmd, args) {
      try {
        return Promise.resolve(handle(cmd, args));
      } catch (error) {
        return Promise.reject(error);
      }
    },
  };
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
    unregisterListener(event, id) {
      listeners.get(event)?.delete(id);
    },
  };
  window.__PULSE_SHOWCASE_UNHANDLED__ = unknown;
})();
