# Kubri

A fast, simple and good-looking Kubernetes GUI that aims to cover everything [k9s](https://k9scli.io) can do. It runs as a desktop app or in your browser.

> **Status:** early development. See the [roadmap](docs/ROADMAP.md) for what comes next.

## Features

- Uses your existing kubeconfig and switches contexts from anywhere.
- Live lists of every resource type, including CRDs, with the same columns as `kubectl get` (press <kbd>w</kbd> for the wide view), sorting, filtering (`-l app=web` for labels) and CPU/memory from metrics-server.
- A command palette like k9s: <kbd>:</kbd> then `pods`, `deploy kube-system`, `po shop app=web`, `apps/v1/deployments`, `ctx prod`, `ns default` or any short name.
- Describe view with containers, conditions and events; YAML view and editor.
- Delete (also several at once), scale, rollout restart, and edit labels and annotations.
- Live logs of a pod, or of every pod behind a deployment, service or other workload, with all containers merged, previous logs, timestamps and a regex filter.
- Shell into any container or attach to its main process, in a real terminal (xterm.js).
- Port-forwards to pods, workloads and services (named target ports included), listed under `:pf`.
- Node maintenance: cordon, uncordon and drain with the same safety checks as `kubectl drain`.
- Pulses (`:pulses`): a cluster overview with healthy, warning and failed counts per kind, CPU and memory use against requests and capacity with a 10-minute trend, and the latest warning events.
- XRay (`:xray deploy`): a tree from each deployment, statefulset, daemonset, job, cronjob, service or ingress down to its pods and containers and the ConfigMaps, Secrets, service accounts and volume claims they use. Missing references show up in red; Secrets are only listed by name, never read.
- RBAC (`:rbac`): every user, group and service account with the roles bound to them and what those roles allow, as a matrix of verbs per resource, plus what your own credentials may do. Roles and bindings also show their rules and subjects in the describe view.
- Helm (`:helm`): the releases in the cluster with their status, chart and app version; their notes, values (as given or merged with the chart defaults), manifest, objects and history, including older revisions. Rolling back and uninstalling use the `helm` command, so they need it installed where Kubri runs.
- Sanitizer (`:popeye`): checks the namespace (or the whole cluster) for common problems like Popeye does, such as missing probes or resource limits, `latest` image tags, crash-looping pods, services without endpoints, unused ConfigMaps, Secrets and roles, and bindings to missing roles. Every kind gets a score and the cluster a grade from A to F.
- HTTP benchmarks (`:bench`, or <kbd>b</kbd> on a port-forward): load-test a forwarded port with a set number of requests or for a set time, then see requests per second, latency percentiles, a histogram of response times, status codes and errors.
- Your k9s setup (`:config`): aliases, hotkeys, plugins and skins from your k9s files work as they are, per context. Plugins run on the machine Kubri runs on, in a terminal over the window or in the background, with the same variables as in k9s. See [Using your k9s configuration](docs/K9S.md).
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

The web server has no login yet. Keep it on localhost until authentication lands: anyone who can reach it can open shells in your cluster, port-forwards and run your k9s plugins on the server's machine. It does turn away requests from other websites, and while it listens on localhost, requests under any other host name, so a web page you visit cannot use it.

Checks that CI runs:

```sh
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
cd ui && pnpm check && pnpm test
```

## License

Kubri is licensed under the [Apache License 2.0](LICENSE).
