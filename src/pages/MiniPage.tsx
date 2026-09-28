import { openMiniWindow } from '@/overlay/desktop';

/**
 * Mini is a small, ordinary PULSE window showing one dashboard — interactive,
 * decorated, never always-on-top. For a window that stays above games and lets
 * clicks through, use an overlay instead.
 */
export function MiniPage() {
  return (
    <section className="page">
      <h1 className="page__title">Mini</h1>
      <p className="page__subtitle">A small, normal PULSE window showing one of your dashboards.</p>
      <div className="card">
        <p className="card__muted">
          Mini is interactive and behaves like any other window. For something that stays on top of
          other applications and lets clicks through, create an overlay in <strong>Overlays</strong>
          .
        </p>
        <div className="card__footer">
          <span />
          <button
            type="button"
            className="button"
            onClick={() => void openMiniWindow().catch(() => undefined)}
          >
            Open Mini window
          </button>
        </div>
      </div>
    </section>
  );
}
