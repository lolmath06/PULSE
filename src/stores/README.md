# Stores

Client-side state: dashboard layouts, widget configuration, user preferences,
active mode.

Empty in Phase 0, and deliberately so — no state management library is chosen
yet. That decision belongs to the phase that first needs shared mutable state,
when the actual requirements are visible.

Two constraints are already known:

- **Dashboard and widget configuration is user data.** It must be serialisable
  and versioned, because it will be saved, exported and shared as presets.
- **Metric samples do not belong here.** Sampling and history live in Rust so
  they survive the main window being hidden (see
  [`../../docs/architecture/overview.md`](../../docs/architecture/overview.md)).
