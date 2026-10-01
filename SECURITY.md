# Security Policy

## Supported versions

PULSE is in early development (Phase 0, `0.1.0-dev`). There is no released
version yet and therefore no long-term support commitment. Security fixes are
applied to `main`.

| Version              | Supported |
| -------------------- | --------- |
| `0.1.x-dev` (`main`) | ✅        |

## Reporting a vulnerability

**Please do not open a public issue for a security vulnerability.**

Report it privately through
[GitHub Security Advisories](https://github.com/lolmath06/PULSE/security/advisories/new).

Please include:

- a description of the issue and its impact;
- the affected platform (Windows / Fedora Linux) and version;
- steps to reproduce;
- any suggested mitigation.

You can expect an acknowledgement within a few days and an assessment shortly
after. Please allow a reasonable period for a fix before public disclosure.

## Security posture

A system monitor reads privileged information about the machine it runs on, so a
few principles are built into the architecture from the start.

### Privilege

**PULSE must be useful without root or administrator rights.** Anything
requiring elevation — SMART attributes, MSR-based temperatures, some sensor
chips, some GPU counters — is an _optional_ capability, clearly labelled in the
UI, never silently requested, and never a hard requirement.

PULSE does not, and will not, silently install kernel drivers or modify system
configuration. Should ring-0 sensor access ever be considered on Windows (see
[`docs/platforms/windows.md`](docs/platforms/windows.md)), it will be an
explicit, documented, opt-in decision — not an implementation detail.

### Attack surface

- **Tauri capabilities are scoped narrowly.** `src-tauri/capabilities/` grants
  only what a window actually needs. Permissions are added when a feature
  requires them, not pre-emptively.
- **A Content Security Policy is enforced** on the webview, configured in
  `tauri.conf.json`.
- **The IPC surface is deliberately small.** Commands are the only channel from
  the UI to the system, they are enumerable in `src-tauri/src/commands/`, and
  they stay thin.
- **Command inputs are treated as untrusted.** Any path or identifier coming
  from the frontend is validated in Rust before use.

### Data

PULSE collects system metrics for display on the user's own machine. It has no
telemetry, sends nothing to any server, and stores no personal data. Any future
feature that would transmit data off the machine must be opt-in and documented.

### Secrets

No credentials, signing keys or tokens belong in this repository. `.gitignore`
excludes the usual offenders (`.env`, `*.pem`, `*.key`, Tauri updater keys), but
the real safeguard is not committing them in the first place.
