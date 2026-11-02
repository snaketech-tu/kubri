# Kubri and Freelens

[Freelens](https://github.com/freelensapp/freelens) is the community fork of Open Lens, the
open-source core of Lens Desktop. This document shows how it differs from Kubri: what Freelens
does that Kubri does not, what Kubri does that Freelens does not, and how the two are built and
shipped. It was written on 2026-11-02 against Freelens 2.0.0 (main at `ac595ba`, version
`2.0.1-0`) from its source and docs, plus its two official extensions that matter most here,
[freelens-ai](https://github.com/freelensapp/freelens-ai) and
[freelens-resource-map-extension](https://github.com/freelensapp/freelens-resource-map-extension).
Kubri is described as of 1.4.2.

The [Headlamp comparison](HEADLAMP-COMPARISON.md) and [IDEAS.md](IDEAS.md) already led to most of
what Kubri 0.3 to 1.4 added, so many gaps that Lens-style tools used to have over Kubri are closed.
What is left is mostly about how clusters are organized, the local terminal, extensions and
distribution.

## At a glance

| | Freelens 2.0 | Kubri 1.4 |
| --- | --- | --- |
| Lineage | Fork of Open Lens (Lens since 2020), MIT | New project, Apache-2.0 |
| Model | Mouse-first desktop app with a cluster catalog, hotbars and a bottom dock | k9s keyboard model plus full mouse support; inspector beside the list and a workspace dock |
| Stack | Electron 44, React, MobX, Monaco; a Go proxy (`freelens-k8s-proxy`) per cluster, bundled `kubectl` and `helm` | Rust core on kube-rs, Tauri 2 desktop, axum server, Svelte UI, CodeMirror |
| Runs as | Desktop only | Desktop, or a web app (Docker or in the cluster) with a sign-in |
| Platforms | macOS (Intel, Apple silicon), Linux (x64, arm64), Windows (x64, arm64) | macOS (Intel, Apple silicon), Linux (x64, arm64), Windows (x64) |
| Packages | DMG, PKG, DEB, RPM, AppImage, NSIS, MSI, portable EXE; Homebrew, WinGet, Scoop, Flathub, Snap, an APT repository, AUR | DMG, DEB, RPM, AppImage, MSI, NSIS, Docker image; Homebrew, winget, AUR and Flatpak are filled in by the release workflow and published where their tokens are set |
| Updates | Through the package manager; the app has no updater | Signed in-app updater (since 1.4.2) |
| Clusters | Several connected at once, each in its own frame, switched with the hotbar | One connected context; a fleet page and search look at several |
| Extensions | Full code extensions (TypeScript, any UI), installed from npm by name, a tarball or a `freelens://` link | Declarative YAML views for CRDs, k9s plugins (shell commands) |
| AI | Optional extension: a chat agent that reads and, after approval, changes resources; OpenAI-compatible APIs only; can use MCP servers | Built in: Explain with your own model, with a preview of what is sent; Kubri as an MCP server for outside agents |
| Community | Core team of four, release team, Discord, Reddit, bounties, an adopters list | One maintainer |

## Where Freelens does better

| # | Area | Freelens | Kubri today | Priority |
| --- | --- | --- | --- | --- |
| 1 | Several clusters open | Each cluster keeps its connection and frame; switching is instant | Switching context reconnects and reloads | Medium |
| 2 | Organizing clusters | Catalog, several named hotbars with icons, kubeconfig folders that are watched for new files | Kubeconfig files, contexts added in the app, a name, label and color per context | Medium |
| 3 | Local terminal | A local shell per cluster with `KUBECONFIG`, a matching `kubectl` and `helm` already set | Shells only into pods and nodes | High |
| 4 | Extensions | Code extensions that add pages, menus, details, columns, settings and themes | YAML views and k9s plugins | Low (by choice) |
| 5 | AI chat | A chat agent with tools for any kind, changes after approval, per-cluster history | Explain one object; agents from outside over MCP | Low |
| 6 | Distribution | Windows arm64, portable EXE, Snap, Scoop, APT repository, all store listings live | No Windows arm64; store listings depend on tokens | Medium |
| 7 | Network settings | App-wide HTTP proxy, "allow untrusted certificates", a proxy per cluster | `HTTPS_PROXY`, `NO_PROXY` and `proxy-url` from the kubeconfig | Low |
| 8 | Metrics sources | Presets for Lens, Helm, Prometheus Operator, Stacklight and OpenShift Prometheus, direct URL with bearer token, GET or POST | Finds Prometheus, Grafana and Loki itself or takes a URL; Grafana token | Low |
| 9 | Preferences | Editor font, size, tab size, minimap and line numbers; terminal font, theme, shell and copy on select; time zone | Zoom, theme and k9s skins | Low |
| 10 | RBAC editing | Dialogs to create RoleBindings and ClusterRoleBindings with subject pickers; a kubeconfig for a service account | RBAC matrix and a suggested Role for a missing permission; bindings through Create | Low |
| 11 | Maturity | Years of Lens users, many adopters, a public extension list | New, few users so far | n/a |

## Details

### 1. Several clusters open at once

Freelens connects each cluster through its own local proxy and shows it in its own frame. Once a
cluster is open, switching to it from the hotbar is instant and its lists, logs and terminals are
still there. Kubri keeps one context connected; `:ctx` reconnects, and its watches start again.
The fleet page, the cross-cluster compare and `:search` look at several contexts, but you work in
one.

**Adopt:** keep the last two or three contexts warm in the core (watches paused, caches kept), so
switching back is instant. Merged multi-cluster lists stay out of scope, as agreed for plan v2.

### 2. Catalog and hotbars

Freelens has a catalog of entities (clusters and web links, extensions add more) and several
named hotbars to which you pin clusters, each with a short name and an icon. Under Preferences >
Kubernetes it can watch folders and files for kubeconfigs, so a new file dropped into
`~/.kube/clusters/` shows up by itself.

Kubri reads `KUBECONFIG` and `~/.kube/config`, can add contexts from a kubeconfig in the app, and
has a name, label and color per context. **Adopt:** watched kubeconfig folders, a cheap win for
people whose cloud CLI writes one file per cluster. Hotbars add little beyond the fleet page and
the context switcher.

### 3. A local terminal for the cluster

"Cluster: Open terminal" in Freelens opens a shell on your own machine in the bottom dock, with
`KUBECONFIG` pointing at the open cluster through Freelens, a `kubectl` that matches the cluster's
version (downloaded when needed) and the bundled `helm` first on `PATH`. Shell, font and theme are
set in the preferences, and on Windows it can run WSL. It is where people run the one command the
GUI does not have.

Kubri opens shells in containers and on nodes, not on your machine. **Adopt:** a "Local terminal"
tab in the workspace for the desktop app, with `KUBECONFIG` set to a file for the current context
and namespace, so `kubectl` and `helm` follow what Kubri shows. In the web app it would need to be
off by default, since it would be a shell on the server.

### 4. Extensions

Freelens extensions are TypeScript packages that run with full privileges in the main process and
the renderer. They can add cluster pages, sidebar entries, menu items, detail sections, table
columns, preferences, catalog entities, status bar items and themes, and use the bundled
`kubectl` and `helm`. They install from the npm registry by name, from a tarball or with a
`freelens://` link. Version 2.0 broke the v1 API on purpose, so the ecosystem is being ported now;
there is a migration guide and an agent skill for the port.

Kubri chose declarative YAML views for CRDs (columns, health, overview sections, actions, graph
edges) and keeps k9s plugins. That decision (plan v2) still holds: views cover most of what
extensions are used for, without running third-party code next to cluster credentials. Nothing
to adopt now; revisit if users ask for pages that YAML cannot describe.

### 5. AI chat

The Freelens AI extension is a chat panel per cluster. Its agent can list, get, create, patch and
delete any kind, read logs, resize pods, evict, restart and clear finalizers. Every change is shown
as YAML and applied only after you approve it. Sessions survive restarts, show token use and cost,
and a warning event has an "Explain" item. It talks only to OpenAI-compatible APIs (other
providers through a gateway such as LiteLLM, or Ollama) and can use MCP servers. It is an
extension, so it is not there until you install it.

Kubri has Explain (one question about one object, after you see what is sent, with secrets
removed) and is itself an MCP server, so Claude Code or any other agent can work with your
clusters and your permissions, each change allowed by you in Kubri. **Adopt (maybe):** follow-up
questions in the Explain panel. A full agent inside Kubri is not needed while outside agents can
use it over MCP.

### 6. Distribution

Freelens builds Windows arm64 too, ships a portable EXE, and is in Homebrew, WinGet, Scoop,
Flathub, Snap, AUR and its own APT repository. Kubri's release workflow fills in Homebrew, winget,
AUR and Flatpak packages, but publishes each only where its token is set.

**Adopt:** add a Windows arm64 build (Tauri supports `aarch64-pc-windows-msvc`), set the package
tokens so the store listings are live, and consider Scoop and Snap.

### 7. Network settings

Freelens has an app-wide HTTP proxy and an "allow untrusted certificates" switch under
Preferences > Proxy, and a proxy per cluster. Kubri follows kubectl: `HTTPS_PROXY`, `NO_PROXY` and
`proxy-url` in the kubeconfig, which is correct but invisible. **Adopt:** show the proxy in use on
the context settings page, and let a context set one. An "untrusted certificates" switch is a
footgun; leave it out.

### 8. Metrics sources

Freelens asks per cluster which Prometheus to use: auto-detect, or one of the presets for the Lens
metrics stack, the Helm chart, Prometheus Operator, Stacklight and OpenShift, or a direct URL with
a bearer token and GET or POST. Its charts appear in pod, node and workload details and the
cluster overview. Kubri finds Prometheus, Grafana and Loki itself or takes their URLs, and draws
the charts against requests and limits. **Adopt:** a bearer token for Prometheus, as Kubri has for
Grafana, and the OpenShift Thanos querier as a known location.

### 9. Preferences

Freelens lets you set the editor's font, size, tab size, minimap and line numbers, the terminal's
font, size, theme and shell, copy on select, and the time zone for timestamps. Kubri has zoom,
light and dark, and k9s skins. **Adopt:** terminal font size and shell for the local terminal
(item 3), and a time zone for timestamps; the rest is low value.

### 10. RBAC editing

Freelens has dialogs to create a RoleBinding or ClusterRoleBinding with pickers for the role and
the users, groups and service accounts, and a menu item that builds a kubeconfig for a service
account. Kubri shows who may do what as a matrix and writes a Role for a missing permission, and
bindings can be made through Create. **Adopt (later):** a binding form in Create, and "kubeconfig
for this service account", which is handy for CI setups.

### 11. Maturity

Lens has been the default Kubernetes GUI for many people since 2020, and Freelens inherits that
reach: an adopters list, a public extension list, packages in every store, and a team that
releases often. Not a feature to copy, but it is why people will compare Kubri with it.

## Where Kubri is ahead

These are things Kubri does that Freelens (with its official extensions) does not:

- **The keyboard.** The k9s keys, `:` command palette for every kind and command, filters with
  regex, `!`, `-f` and `-l`, and the user's k9s aliases, hotkeys, plugins, skins and settings.
  Freelens has a command palette for navigation and a handful of shortcuts.
- **The browser.** Kubri runs as a web app from Docker or in the cluster. Freelens is desktop only.
- **Size and speed.** A Tauri app with a Rust core that watches and builds tables on the server
  side, 10,000 pods in about a second, measured in CI. Freelens is an Electron app with a Go proxy
  per cluster, and the renderer holds the objects.
- **Safety.** A name, color and label per context, a production banner that asks for the name
  before deleting, read-only and locked contexts enforced in the core, a dry-run diff before
  changes, every action as a `kubectl` command, and "what breaks" before deleting something in
  use. Freelens asks for confirmation and has no read-only mode.
- **Permissions that explain themselves.** Actions you may not run say which permission is missing
  and offer a Role to send to an admin; lists fall back to readable namespaces on their own.
  Freelens hides sidebar items you cannot list, and when listing namespaces is forbidden it shows
  only the context's namespace unless you type the accessible ones into the cluster settings.
- **Editing.** Field docs, completion and checks from the cluster's schema, and changes made by
  others meanwhile merged into yours. Freelens uses Monaco with plain YAML and applies the edit.
- **Rollouts.** Set image, rollout history and rollback for Deployments, clone to another
  namespace or context. Freelens has scale and restart; rollback only for Helm releases.
- **Logs.** Structured logs (JSON and logfmt fields as columns, `status>=500`), levels, folded
  stack traces, a rate strip, new pods joining during a rollout, and two logs or shells side by
  side. Freelens has search, timestamps, previous container, wrap and download, with a pod picker
  for workloads.
- **Debugging.** Debug containers, debug copies of crash-looping pods, and a fix with each likely
  problem (memory limit with its diff, why a pod is pending on each node, why a Service has no
  endpoints). Freelens has node shells and shells into existing containers.
- **Whole-cluster views.** Pulses, XRay and the namespace graph (Freelens needs the resource map
  extension for a graph), the RBAC matrix, a Popeye-style sanitizer, HTTP benchmarks over
  port-forwards, right-sizing, and a fleet page with cross-cluster compare and search.
- **History.** "What changed" timeline with diffs, and snapshots of a namespace or cluster opened
  offline.
- **Integrations.** Flux, Argo CD and cert-manager status and actions, with a warning before
  changing what they manage, and Grafana dashboards and Loki history next to Prometheus.
- **Helm.** Install from repositories, OCI and Artifact Hub, upgrade with a diff of every object,
  compare revisions and spot drift. Freelens installs, upgrades and rolls back with a values
  editor.
- **Port-forwards.** Favorites that start with Kubri and follow a restarted pod. Freelens keeps
  its port-forwards across restarts and opens them in the browser.
- **Updates.** A signed updater in the app.

## Same in both

Every kind and CRD with its printer columns, live lists, namespace filter, CPU and memory in the
pod list, multi-select with bulk delete, column chooser, details with events, decoded Secrets,
create from templates, YAML edit, shell and attach, node shell, cordon, uncordon and drain,
scale and restart, CronJob trigger and suspend, port-forwards, Helm releases with rollback and
uninstall, light and dark themes, and Prometheus charts.

## Suggested order

1. Local terminal per context in the desktop app (item 3), with terminal font and shell settings
   (item 9).
2. Windows arm64 build and live store listings (item 6).
3. Watched kubeconfig folders (item 2) and warm switching between recent contexts (item 1).
4. Proxy shown and set per context, Prometheus bearer token (items 7 and 8).
5. Later: binding form and service account kubeconfig (item 10), follow-up questions in Explain
   (item 5), time zone setting.
