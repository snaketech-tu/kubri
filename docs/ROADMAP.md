# Roadmap

Kubri aims for feature parity with k9s. The work is split into four steps that build on each other.

## 1. Foundation ✅

- Rust workspace with a shared core on kube-rs: kubeconfig loading (merged `KUBECONFIG`), contexts, API discovery including CRDs, live watches.
- Tauri 2 desktop app and axum web server, both serving the same SvelteKit UI.
- Global context switcher, namespace picker, live resource list.
- CI, Apache 2.0 license.

## 2. Resource browser ✅

- Global context switcher everywhere, including `:ctx` in the command palette.
- Lists and details for every resource type, with the columns `kubectl get` shows.
- Namespace switching, filtering and search, command palette (`:pods`, `:deploy`, …), full keyboard control.
- View and edit YAML, describe, events.
- Actions: delete, scale, restart, labels and annotations.
- CPU and memory metrics from metrics-server.

## 3. Pod and node tools ✅

- Live log streaming for pods, workloads and services, with filtering, multiple containers, previous logs.
- Shell into containers (xterm.js), attach.
- Port forwarding to pods, workloads and services, with a list of running forwards.
- Node cordon, uncordon and drain.

## 4. Advanced features and release ✅

- ✅ XRay (resource relationships) and Pulses (cluster dashboard).
- ✅ RBAC view, Helm releases.
- ✅ Cluster sanitizer (Popeye), HTTP benchmarks.
- ✅ Plugins, hotkeys, aliases and skins, compatible with k9s configuration files.
- ✅ Packages for Windows, macOS and Linux; Docker image for the web mode with login; documentation.

## Next: beyond Headlamp

The plan for 0.3 to 1.0 is in [IMPLEMENTATION-PLAN-v2.md](IMPLEMENTATION-PLAN-v2.md).

- ✅ 0.3 Work safely: context names, colors and guard rails, permission-aware actions, copy as kubectl, dry-run diffs, editor help.
- ✅ 0.4 AI: Kubri as an MCP server for AI agents, and Explain with your own model.
- ✅ 0.5 Logs and debugging: structured logs, a workspace of log and shell tabs, debug containers and node shells, and fixes from the likely problem.
- ✅ 0.6 Create and change: Create from templates, files and URLs with a dry run, forms and cloning, Helm install, upgrade, compare and drift, columns, bulk changes and port-forwards that stay.
