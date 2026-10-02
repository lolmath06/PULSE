# Contributing to PULSE

Thank you for considering a contribution. PULSE is young, and the priority right
now is **architectural quality over feature count**. A small, well-placed change
is worth more than a large one that blurs a boundary.

## Before you start

Read [`docs/architecture/overview.md`](docs/architecture/overview.md). It is
short, and it explains the boundaries that the rest of this document assumes.

## The cross-platform rule

PULSE targets **Windows and Fedora Linux as two first-class platforms**.

> A system-facing feature is not considered complete until its behavior on both
> Windows and Fedora Linux has been designed and, whenever materially testable,
> validated.

In practice, for any change that reads the system:

- implement it, or explicitly document its unavailability, on **both** platforms;
- describe the data source and its failure modes in the relevant
  [`docs/platforms/`](docs/platforms/) file;
- state in your pull request which platforms you tested on, and which you did
  not. **Never claim a platform you did not test.** "Compiles in CI, not
  physically tested on Windows" is a perfectly good and honest status.

## Architectural rules

These are not style preferences. A pull request that breaks one of them will be
asked to change.

1. **The frontend never touches the system.** No `/proc`, `/sys`, `hwmon`, WMI,
   PDH, NVML or vendor SDK from React. Everything crosses the boundary as a
   Tauri command returning a structured payload.
2. **Only `src-tauri/src/platform/` branches on the OS.** `#[cfg(target_os =
"...")]` for OS selection belongs there and nowhere else. Services and
   commands depend on the `HostPlatform` trait.
3. **Platform-only crates are declared per target**, under
   `[target.'cfg(target_os = "...")'.dependencies]`.
4. **Only `src/services/` calls `invoke`.** Components and hooks call services.
5. **Commands stay thin.** Validate, delegate to a service, return. Logic in a
   command is logic that cannot be unit-tested.
6. **Rust payloads are mirrored in `src/types/`**, serialised as camelCase.
7. **Parsing and detection logic takes its input as arguments**, so it can be
   tested on a machine that is not the one it describes. See
   `detect_display_server` and `format_os_version` for the pattern.

## Development workflow

```bash
pnpm install
pnpm app:dev
```

Before opening a pull request:

```bash
pnpm check:all
```

That runs format checks, ESLint, TypeScript, Vitest, `cargo fmt --check`,
Clippy with warnings denied, and `cargo test` — the same things CI runs.

## Adding a platform capability

Always the same three steps:

1. Add a method to the `HostPlatform` trait, with a safe default if one exists.
2. Implement it in `platform/linux/` **and** `platform/windows/`.
3. Expose it through a service and a command, and mirror the payload type in
   `src/types/`.

## Commits

[Conventional Commits](https://www.conventionalcommits.org/):

```text
feat: add CPU frequency collector
fix(linux): handle missing hwmon label files
docs(platforms): document Windows temperature constraints
refactor(platform): extract display server detection
test(metrics): cover counter rollover
chore(ci): cache cargo registry
```

Keep the subject in the imperative mood and under ~72 characters.

## Pull requests

- One logical change per pull request.
- Fill in the template, including the platform testing section.
- Add tests for logic that can be tested without the hardware it describes —
  which, if you follow rule 7 above, is most of it.
- Update the documentation in the same pull request as the code.
- Add a `CHANGELOG.md` entry under `[Unreleased]` for user-visible changes.

## Scope

PULSE is developed in phases, and staying inside the current phase matters.
If an idea belongs to a later phase, open an issue describing it rather than
implementing it early — premature features are how architectures get bent out of
shape. The current phase is stated in [`README.md`](README.md#status).

## Code of conduct

Be direct, be kind, assume good faith. Critique code, not people.

## Contributions and licensing

PULSE is proprietary software. Submitting a contribution does not change the
project's license. By submitting a contribution, you grant the copyright holder
the right to use, modify and distribute it as part of PULSE under the PULSE
license. See [LICENSE](LICENSE).
