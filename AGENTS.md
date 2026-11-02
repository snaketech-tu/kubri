# AGENTS.md

Kubri is a Kubernetes GUI for desktop and browser that covers what k9s does, by keyboard or mouse. One Rust core talks to the cluster; a Tauri desktop app and an axum web server both expose it to one Svelte UI.

`docs/AGENTS.md` is user documentation for Kubri's MCP server, not instructions for you.

## Layout

| Path | What it is |
| --- | --- |
| `crates/kubri-core` | All cluster access (kube-rs). `Kubri` in `src/lib.rs` owns the merged kubeconfig and the connection; features add `impl Kubri` blocks in their own module. Built-in views for custom resources live in `views/`. |
| `crates/kubri-server` | Web mode: axum REST + WebSocket API (`api.rs`), sign-in (`auth.rs`), Host/Origin guard (`guard.rs`), serves `ui/build`. |
| `crates/kubri-mcp` | MCP server (stdio and HTTP) on top of the core. |
| `ui/src-tauri` | Desktop app (crate `kubri-desktop`, binary `kubri`): Tauri commands in `src/lib.rs` call the core directly. |
| `ui` | SvelteKit + TypeScript (Svelte 5 runes), static SPA shared by both apps. |
| `ui/src/lib/api` | The `Backend` interface (`types.ts`) with two implementations: `http.ts` (web) and `tauri.ts` (desktop). |
| `docs/` | User documentation, linked from the README. |
| `packaging/`, `.github/workflows/release.yml` | Installers, package-manager templates, updater `latest.json`. |
| `bench/speed`, `scripts/` | Speed benchmark against k3s; Playwright UI checks and README screenshots. |

A new backend call touches every layer: a method on `Kubri` in the core, a route in `crates/kubri-server/src/api.rs`, a command in `ui/src-tauri/src/lib.rs` (registered in `generate_handler!`), the method in `Backend` plus both `http.ts` and `tauri.ts`. Keep both apps behaving the same.

## Commands

Prerequisites: stable Rust, Node.js 22, pnpm 10 (`packageManager` in `ui/package.json`).

```sh
cd ui && pnpm install --frozen-lockfile
pnpm check            # svelte-check + types
pnpm test             # vitest
pnpm build            # writes ui/build

# from the repository root
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

These are exactly what CI (`.github/workflows/ci.yml`) runs. Notes:

- Run `pnpm build` before building `kubri-desktop`: Tauri embeds `ui/build` at compile time and fails if it is missing.
- `kubri-desktop` needs the Tauri system libraries (`libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev libayatana-appindicator3-dev librsvg2-dev`). Without them, check the rest with `--exclude kubri-desktop` and say that the desktop crate was not checked.
- CI uses the newest stable Rust, whose clippy can have lints an older local toolchain lacks. Run `rustup update stable` before clippy, or a change that passes locally can fail CI.
- The UI has no formatter config. Do not run Prettier or similar on `ui/`; it would rewrite every file. Match the surrounding style (tabs, single quotes).
- Rust formatting: `rustfmt.toml` sets `max_width = 110`.

Running it:

```sh
pnpm tauri dev                         # in ui/: desktop app with hot reload
cargo run -p kubri-server              # root, plus `pnpm dev` in ui/: browser on http://localhost:5173 (Vite proxies /api)
cargo run --release -p kubri-server    # root, after `pnpm build`: production UI on http://127.0.0.1:7878
```

`kubri-server --help` lists its flags (all also read from `KUBRI_*` variables, e.g. `--read-only`, `--ui-dir`).

## Tests

- Rust: unit tests in a `mod tests` at the bottom of the module. `kubri_core::testing::client` is a fake API server that answers and records requests; prefer it over a real cluster.
- UI: `ui/src/lib/*.test.ts` next to the module they test. Keep logic in plain `.ts` modules so it can be tested without rendering components.
- Browser check against the production UI with fixtures: `pnpm build && pnpm preview --port 4189` in `ui/`, then `node scripts/check-ui.cjs <path-to-playwright-module>`.
- Real cluster: k3s with `--disable=traefik --write-kubeconfig-mode=644`, kubeconfig at `/etc/rancher/k3s/k3s.yaml` (see the `speed` job in `ci.yml`). Use it for anything that only shows against a cluster: logs, exec, port-forward, watches, RBAC.

## Conventions

- **Actions** on objects come from one list, `objectActions` in `ui/src/lib/objects.ts`. The action bar, row menu, details panel, command palette and keys all read it. Add an action there, not in a single view. The menu model is `MenuAction` in `ui/src/lib/menu.ts`; side effects live in `ui/src/lib/actions.ts`.
- **Changes to a cluster** go through the core and call `ensure_writable()` (read-only contexts, `--read-only`, k9s `readOnly`, snapshots, AI-agent rules) and, where an RBAC check fits, `ensure_may()`. Never enforce a guard rail only in the UI.
- **Kubeconfig loading** (`crates/kubri-core/src/kubeconfig.rs`) deliberately mirrors client-go's loader, validation and transport, with kubectl's error messages: a context must work in Kubri exactly when it works in kubectl. Check client-go before changing behavior there.
- **k9s compatibility**: keys, aliases, hotkeys, plugins and skins follow k9s (`crates/kubri-core/src/k9s.rs`, `ui/src/lib/k9s.ts`, `docs/K9S.md`). Do not break an existing k9s key.
- **Errors**: core functions return `kubri_core::Result` with `Error` from `error.rs`. Messages reach users as they are: lowercase, no trailing period, saying what to do where possible ("not connected to a cluster, select a context first").
- **Dialogs** are modal `<dialog>`s with `{@attach lightDismiss}` (`ui/src/lib/dismiss.ts`), so a click outside closes them like Esc.
- Keep changes focused. Update the matching page in `docs/` when user-visible behavior changes.

## Writing

- User-facing text (UI, docs, errors, release notes) is plain, short English sentences, without jargon.
- Never compare Kubri with other tools or name competitors in the repository, PRs or commit messages. Describing k9s compatibility, as the README does, is fine.
- Do not link internal implementation plans (`docs/IMPLEMENTATION-PLAN*.md`) from the README.

## Git and pull requests

- Commit as `snake.tech <tolga.urhan@hotmail.de>` (`git config user.name "snake.tech"` and `user.email` in a fresh clone).
- No AI attribution anywhere: no `Co-Authored-By` or session trailers, no "Generated with" footers in commits, PR bodies or comments, no tool-named branch prefixes. Branches use `feat/`, `fix/`, `docs/` or `release/`.
- Branch off `main` and do not stack PRs: PRs are squash-merged and a stacked PR's base does not retarget.
- Commit and PR titles describe the change in plain words (e.g. "Load kubeconfig files in the app").
- Leave merging to the maintainer unless asked.
- Before reporting work as done, review your own diff (correctness, edge cases, light and dark theme, keyboard flow, tests, docs), fix what you find and rerun the checks.

## Releases

See `docs/RELEASING.md`. In short: set the version in `Cargo.toml` (`[workspace.package]`), `ui/package.json` and `ui/src-tauri/tauri.conf.json`, run `cargo check` so `Cargo.lock` follows, add a `CHANGELOG.md` entry, merge to `main`, then tag `vX.Y.Z` on the merge commit. The Release workflow refuses a tag that does not match `Cargo.toml`; it and the Docker workflow build the installers and the image.
