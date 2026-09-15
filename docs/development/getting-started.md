# Getting Started

## Prerequisites

Common to both platforms:

- **Node.js 20.19+** (22 LTS recommended — see `.nvmrc`)
- **pnpm 10+** — `corepack enable pnpm`
- **Rust stable 1.77.2+** — via [rustup](https://rustup.rs)

### Fedora Linux

```bash
sudo dnf install -y \
  webkit2gtk4.1-devel \
  openssl-devel \
  curl wget file \
  libappindicator-gtk3-devel \
  librsvg2-devel \
  gcc gcc-c++ make
```

### Windows

- **Microsoft C++ Build Tools** with the "Desktop development with C++" workload
- **WebView2 Runtime** (preinstalled on Windows 11 and current Windows 10)

## Setup

```bash
cd PULSE
pnpm install
```

## Running

```bash
pnpm app:dev     # the real PULSE desktop app (Tauri + Rust backend)
pnpm dev         # UI only, in a browser — no backend, for fast styling work
```

`pnpm dev` is genuinely useful for CSS work, but the backend is unreachable:
the status bar will say _"Backend unavailable — UI-only mode"_. That is expected,
not a failure.

### Development ports

PULSE serves its dev frontend on **`http://localhost:1421`**, with the HMR
websocket on **1422**.

These are deliberately not Tauri's default `1420`. That default is shared by
every Tauri project, so two of them cannot run at the same time — and PULSE must
never require another project to be shut down before it can start. With a
dedicated port, PULSE runs alongside your other Tauri/Vite work.

The port is fixed (`strictPort`) because Tauri's `devUrl` is a fixed address: a
silent fallback to another port would leave the webview loading nothing. To
change it, edit `DEV_SERVER_PORT` in `vite.config.ts` **and** `build.devUrl` in
`src-tauri/tauri.conf.json` together — they must always agree.

## Commands

| Command                             | What it does                                   |
| ----------------------------------- | ---------------------------------------------- |
| `pnpm app:dev`                      | Run PULSE in development (Tauri + Vite + Rust) |
| `pnpm app:build`                    | Build the desktop application                  |
| `pnpm dev`                          | Vite dev server only (no backend)              |
| `pnpm build`                        | Typecheck and build the frontend bundle        |
| `pnpm typecheck`                    | TypeScript, no emit                            |
| `pnpm lint` / `pnpm lint:fix`       | ESLint                                         |
| `pnpm format` / `pnpm format:check` | Prettier                                       |
| `pnpm test` / `pnpm test:watch`     | Vitest                                         |
| `pnpm rust:fmt`                     | `cargo fmt --check`                            |
| `pnpm rust:lint`                    | `cargo clippy --all-targets -D warnings`       |
| `pnpm rust:test`                    | `cargo test`                                   |
| `pnpm check:all`                    | Everything CI runs, in one command             |

Run `pnpm check:all` before opening a pull request.

## Project layout

```text
PULSE/
├── src/                 # React frontend
│   ├── app/             # composition root: router, routes, constants
│   ├── components/      # reusable components
│   ├── features/        # feature slices (UI + state + logic)
│   ├── hooks/
│   ├── layouts/         # page frames
│   ├── pages/           # one component per route
│   ├── services/        # the invoke() boundary — the ONLY caller of Tauri
│   ├── stores/          # client state
│   ├── styles/          # design tokens and global CSS
│   ├── types/           # shared types, mirrors of Rust payloads
│   └── utils/
├── src-tauri/           # Rust backend
│   ├── src/
│   │   ├── commands/    # Tauri command surface
│   │   ├── metrics/     # metrics engine (future)
│   │   ├── platform/    # THE platform seam
│   │   │   ├── linux/   # /proc, /sys, hwmon, Wayland/X11
│   │   │   └── windows/ # WMI, PDH, vendor SDKs
│   │   ├── services/    # cross-platform application logic
│   │   ├── state/       # shared application state
│   │   ├── lib.rs
│   │   └── main.rs
│   ├── capabilities/    # Tauri permission scoping
│   └── tauri.conf.json
├── docs/
└── .github/workflows/   # CI
```

Some folders are still empty. That is intentional: the architecture is fixed,
the content arrives with the features.

## Conventions

- **The frontend never touches the system.** No `/proc`, `/sys`, WMI, PDH or
  NVML from React — ever. Everything goes through a Tauri command.
- **Only `src-tauri/src/platform/` uses `#[cfg(target_os = ...)]`** for OS
  selection. Services and commands depend on the `HostPlatform` trait.
- **Only `src/services/` calls `invoke`.** Components and hooks call services.
- **Platform-only crates go under `[target.'cfg(target_os = "...")'.dependencies]`.**
- **Rust payloads are mirrored in `src/types/`**, serialised as camelCase.

Commit messages follow [Conventional Commits](https://www.conventionalcommits.org/).
