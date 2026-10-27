# Kubri

A fast, simple and good-looking Kubernetes GUI that aims to cover everything [k9s](https://k9scli.io) can do. It runs as a desktop app or in your browser.

> **Status:** early development. See the [roadmap](docs/ROADMAP.md) for what comes next.

## Features

- Uses your existing kubeconfig and switches contexts from anywhere.
- Live lists of every resource type, including CRDs, with the same columns as `kubectl get` (press <kbd>w</kbd> for the wide view), sorting, filtering (`-l app=web` for labels) and CPU/memory from metrics-server.
- A command palette like k9s: <kbd>:</kbd> then `pods`, `deploy kube-system`, `ctx prod`, `ns default` or any short name.
- Describe view with containers, conditions and events; YAML view and editor.
- Delete (also several at once), scale, rollout restart, and edit labels and annotations.
- Fully usable from the keyboard. Press <kbd>?</kbd> in the app for all shortcuts.

## How it works

Kubri uses the kubeconfig you already have: every file listed in `KUBECONFIG`, or `~/.kube/config` if that is not set. It connects to your `current-context` on startup, and you can switch contexts at any time from the top bar (or with <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>C</kbd>).

| Part | Path | What it is |
| --- | --- | --- |
| Core | [`crates/kubri-core`](crates/kubri-core) | All Kubernetes access, built on [kube-rs](https://kube.rs). Shared by both apps. |
| Web server | [`crates/kubri-server`](crates/kubri-server) | Serves the UI and a REST + WebSocket API on `127.0.0.1:7878`. |
| Desktop app | [`ui/src-tauri`](ui/src-tauri) | [Tauri 2](https://tauri.app) shell that calls the core directly. |
| UI | [`ui`](ui) | SvelteKit + TypeScript single-page app, used by both. |

## Development

You need Rust (stable), Node.js 22 and pnpm. On Linux the desktop app also needs the [Tauri system dependencies](https://tauri.app/start/prerequisites/).

```sh
cd ui && pnpm install

# Desktop app with hot reload
pnpm tauri dev

# Browser: run the server and the Vite dev server (which proxies /api to it)
cargo run -p kubri-server      # from the repository root
pnpm dev                       # in ui/, then open http://localhost:5173
```

To serve a production build in the browser, run `pnpm build` in `ui/`, then `cargo run --release -p kubri-server` from the repository root and open <http://127.0.0.1:7878>.

The web server has no login yet. Keep it on localhost until authentication lands.

Checks that CI runs:

```sh
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
cd ui && pnpm check && pnpm test
```

## License

Kubri is licensed under the [Apache License 2.0](LICENSE).
