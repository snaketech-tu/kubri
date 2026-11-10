# AGENTS.md

Kubri is a Kubernetes GUI for desktop and browser that covers what k9s does, by keyboard or mouse. One Rust core talks to the cluster; a Tauri desktop app and an axum web server both expose it to one Svelte UI.

`docs/AGENTS.md` is user documentation for Kubri's MCP server, not instructions for you.

## Layout

| Path | What it is |
| --- | --- |
| `crates/kubri-core` | All cluster access (kube-rs). `Kubri` in `src/lib.rs` owns the merged kubeconfig and the connection; features add `impl Kubri` blocks in their own module. Built-in views for custom resources live in `views/`. |
| `crates/kubri-server` | Web mode: axum REST + WebSocket API (`api.rs`), sign-in (`auth.rs`, OpenID Connect in `oidc.rs`), a Kubri per signed-in person (`team.rs`), audit log (`audit.rs`), Host/Origin guard (`guard.rs`), serves `ui/build`. |
| `crates/kubri-mcp` | MCP server (stdio and HTTP) on top of the core. |
| `crates/kubri-cli` | `lint` and `snapshot` commands without a window; both `kubri` and `kubri-server` run them when they are the first argument. `actions/lint` is the GitHub Action around it. |
| `ui/src-tauri` | Desktop app (crate `kubri-desktop`, binary `kubri`): Tauri commands in `src/lib.rs` call the core directly. |
| `ui` | SvelteKit + TypeScript (Svelte 5 runes), static SPA shared by both apps. |
| `ui/src/lib/api` | The `Backend` interface (`types.ts`) with two implementations: `http.ts` (web) and `tauri.ts` (desktop). |
| `docs/` | User documentation, linked from the README. |
| `deploy/` | Manifest and Helm chart (`deploy/helm/kubri`) for running kubri-server in a cluster. |
| `packaging/`, `.github/workflows/release.yml` | Installers, package-manager templates, updater `latest.json`. |
| `bench/speed`, `scripts/` | Speed benchmark against k3s; Playwright UI checks and README screenshots. |
| `site/`, `scripts/demo/` | The website, and the demo snapshot `ui/static/demo.kubri` with the recorder for the live demo (`ui/src/lib/api/replay.ts`); `.github/workflows/pages.yml` publishes both. |

Types the UI sends and receives are written from the Rust types by ts-rs: derive `ts_rs::TS` with `#[ts(export)]` next to `Serialize` or `Deserialize`, run `cargo test`, and commit `ui/src/lib/api/generated/` (CI fails when it is out of date). Fields that serde may leave out (`skip_serializing_if`) need `#[ts(optional)]`. For a request the UI may send with fields left out, add `optional_fields = nullable` to the type's `#[ts(...)]`, and `#[ts(as = "Option<T>", optional)]` on a non-`Option` field that has a serde default. `ui/src/lib/api/types.ts` re-exports them and keeps only types that exist nowhere in the core: the `Backend` interface, terminal sessions, Kubernetes objects and the desktop app's updater.

Backend calls are moving to one call table, `crates/kubri-core/src/call.rs`: a call there is one line in the table, the method in `Backend` and one line in `ui/src/lib/api/rpc.ts`, and both apps get it through `POST /api/call` and the desktop `call` command. A call that changes something needs its name in `call_action` in `crates/kubri-server/src/audit.rs`. Calls not moved yet still touch every layer: a method on `Kubri` in the core, a route in `crates/kubri-server/src/api.rs`, a command in `ui/src-tauri/src/lib.rs` (registered in `generate_handler!`), the method in `Backend` plus both `http.ts` and `tauri.ts`. Streams (tables, logs, terminals) keep their own sockets and channels. Keep both apps behaving the same.

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
cargo deny check      # vulnerabilities, licenses, sources (deny.toml); `cargo install cargo-deny`
cd ui && pnpm audit --prod
```

These are exactly what CI (`.github/workflows/ci.yml`) runs. Notes:

- Run `pnpm build` before building `kubri-desktop`: Tauri embeds `ui/build` at compile time and fails if it is missing.
- `kubri-desktop` needs the Tauri system libraries (`libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev libayatana-appindicator3-dev librsvg2-dev`). Without them, check the rest with `--exclude kubri-desktop` and say that the desktop crate was not checked.
- CI uses the newest stable Rust, whose clippy can have lints an older local toolchain lacks. Run `rustup update stable` before clippy, or a change that passes locally can fail CI.
- The UI has no formatter config. Do not run Prettier or similar on `ui/`; it would rewrite every file. Match the surrounding style (tabs, single quotes).
- Rust formatting: `rustfmt.toml` sets `max_width = 110`.
- A newly published advisory can fail the Dependencies job on any PR. Update the crate or package; only if no fix exists, add the advisory to `ignore` in `deny.toml` with a comment saying why Kubri is not affected. A new dependency needs a license from the `allow` list there.

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
- Real cluster: k3s with `--disable=traefik --write-kubeconfig-mode=644`, kubeconfig at `/etc/rancher/k3s/k3s.yaml` (see the `e2e` job in `ci.yml`). Use it for anything that only shows against a cluster: logs, exec, port-forward, watches, RBAC.
- End-to-end: `scripts/e2e/` drives the web app against k3s (`node --test scripts/e2e/kubri.test.cjs`, see its README) and starts the desktop app (`desktop.cjs`). CI runs both. Add a test there when you change a flow that only shows against a cluster.

## Conventions

- **Actions** on objects come from one list, `objectActions` in `ui/src/lib/objects.ts`. The action bar, row menu, details panel, command palette and keys all read it. Add an action there, not in a single view. The menu model is `MenuAction` in `ui/src/lib/menu.ts`; side effects live in `ui/src/lib/actions.ts`.
- **Changes to a cluster** go through the core and call `ensure_writable()` (read-only contexts, `--read-only`, k9s `readOnly`, snapshots, AI-agent rules) and, where an RBAC check fits, `ensure_may()`. Never enforce a guard rail only in the UI.
- **Team mode** (`docs/TEAMS.md`): `Kubri::for_person` makes a Kubri that acts with a signed-in person's own credentials. Anything that would run on the server or use its credentials (adding contexts, plugins, local terminals, agents) refuses with `ensure_not_team()`. A new route that changes something needs an entry in the action table in `crates/kubri-server/src/audit.rs`, so it reaches the audit log.
- **Kubeconfig loading** (`crates/kubri-core/src/kubeconfig.rs`) deliberately mirrors client-go's loader, validation and transport, with kubectl's error messages: a context must work in Kubri exactly when it works in kubectl. Check client-go before changing behavior there.
- **k9s compatibility**: keys, aliases, hotkeys, plugins and skins follow k9s (`crates/kubri-core/src/k9s.rs`, `ui/src/lib/k9s.ts`, `docs/K9S.md`). Do not break an existing k9s key.
- **Errors**: core functions return `kubri_core::Result` with `Error` from `error.rs`. Messages reach users as they are: lowercase, no trailing period, saying what to do where possible ("not connected to a cluster, select a context first").
- **UI text** goes through `t()` from `ui/src/lib/i18n`, with the English text as the key and its German in `de.ts`; `plural()` for counts, and `Phrase.svelte` for a text with a value in bold or as code. Never build a sentence from pieces or compare a translated text in code. `pnpm test` fails when a `t()` text has no German, when German is left over, and when `crates/kubri-core/src/error.rs` changes a message without `ui/src/lib/i18n/errors.ts` (the UI translates the core's errors by their format).
- **Dialogs** are modal `<dialog>`s with `{@attach lightDismiss}` (`ui/src/lib/dismiss.ts`), so a click outside closes them like Esc.
- Keep changes focused. Update the matching page in `docs/` when user-visible behavior changes.

## Writing

- User-facing text (UI, docs, errors, release notes) is plain, short English sentences, without jargon.
- Never compare Kubri with other tools or name competitors in the repository, PRs or commit messages. Describing k9s compatibility, as the README does, is fine.
- Implementation plans are kept outside the repository. `docs/ROADMAP.md` says in plain words what is coming; update it when a release ships or a plan changes.

## Git and pull requests

- Commit as `snake.tech <tolga.urhan@hotmail.de>` (`git config user.name "snake.tech"` and `user.email` in a fresh clone).
- No AI attribution anywhere: no `Co-Authored-By` or session trailers, no "Generated with" footers in commits, PR bodies or comments, no tool-named branch prefixes. Branches use `feat/`, `fix/`, `docs/` or `release/`.
- Branch off `main` and do not stack PRs: PRs are squash-merged and a stacked PR's base does not retarget.
- Commit and PR titles describe the change in plain words (e.g. "Load kubeconfig files in the app").
- Leave merging to the maintainer unless asked.
- Dependabot (`.github/dependabot.yml`) opens grouped dependency updates every Monday. Third-party actions are pinned to a commit with the version in a comment; Dependabot keeps both current.
- Before reporting work as done, review your own diff (correctness, edge cases, light and dark theme, keyboard flow, tests, docs), fix what you find and rerun the checks.

## Releases

See `docs/RELEASING.md`. In short: set the version in `Cargo.toml` (`[workspace.package]`), `ui/package.json`, `ui/src-tauri/tauri.conf.json` and `deploy/helm/kubri/Chart.yaml` (`version` and `appVersion`), run `cargo check` so `Cargo.lock` follows, add a `CHANGELOG.md` entry, write `.github/release-notes.md`, merge to `main`, then run the Release workflow on `main` with `publish` (`gh workflow run release.yml --ref main -f publish=true`, or the Actions API). It refuses a version that is already released, builds the installers and the image into a draft, and publishes it, which creates the tag, once everything is attached.
