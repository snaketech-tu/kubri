# Where Headlamp does better than Kubri

[Headlamp](https://github.com/kubernetes-sigs/headlamp) is the Kubernetes GUI of SIG UI and the
recommended successor of the archived Kubernetes Dashboard. This document lists what it does
better than Kubri today, so we can decide what to adopt. It was written on 2026-10-29 against
Headlamp 0.46.0 (main at `4b3572c`) and its official plugins in
[headlamp-k8s/plugins](https://github.com/headlamp-k8s/plugins), from their docs and source code.

Kubri aims at k9s parity and a keyboard-first workflow, Headlamp at a point-and-click dashboard
for teams. Many of the gaps below follow from that, but several are worth closing anyway. Each
item says what Headlamp does, where Kubri stands, and what we could do.

## Summary

| # | Area | Headlamp | Kubri today | Priority |
| --- | --- | --- | --- | --- |
| 1 | Logs and terminals | Open full size, can be split, minimized and kept running | Narrow side panel, maximize with <kbd>f</kbd> | High |
| 2 | Log viewer | Severity filter, JSON prettify, all pods of a workload with a pod picker | Filter, export, timestamps, wrap, merged workload logs | Medium |
| 3 | Several clusters at once | Pick several clusters, lists and search span them | One context at a time | High |
| 4 | Clusters from the UI | Load a kubeconfig by drag and drop, rename, color, icon, remove | Reads kubeconfig files only | Medium |
| 5 | Create resources | Create from YAML, a file or a URL, guided forms, dry-run preview | Edit only, no create | High |
| 6 | Permission-aware UI | Hides or disables actions the user may not do | Actions fail on the API call | High |
| 7 | Sign-in for the web app | OIDC with guides for Keycloak, Dex, Entra ID, Cognito and more | One shared password, or an auth proxy | High |
| 8 | Plugins | JavaScript plugins that add views, columns, map nodes; in-app catalog | k9s shell-command plugins | Medium |
| 9 | Ecosystem integrations | Official plugins for Prometheus, Flux, Argo CD, cert-manager, KEDA, Karpenter and about 25 more | Helm only | Medium |
| 10 | Install Helm charts | App Catalog from Artifact Hub | Releases, history, rollback, uninstall | Low |
| 11 | Debugging | Ephemeral debug containers, node shell | Shell and attach into existing containers | Medium |
| 12 | Search | Global search, and a query search over many kinds and clusters with saved searches | Command palette, per-list filters | Medium |
| 13 | Resource map | Interactive graph of the whole namespace or cluster, grouped by namespace, node or instance | XRay tree per kind | Low |
| 14 | Projects | Group an application across namespaces and clusters, with its own map and access tab | None | Low |
| 15 | Editor help | API field docs beside the YAML, warns when the object changed meanwhile | YAML editor, unsaved-changes guard | Medium |
| 16 | Tables | Column chooser, rows per page, many bulk actions | Fixed `kubectl get` columns, wide view | Low |
| 17 | Languages | 26 locales | English only | Low |
| 18 | Accessibility and theming | Zoom saved per user, high-zoom layouts, theme picker with previews | k9s skins, light and dark | Low |
| 19 | Distribution | Helm chart, Flatpak, Homebrew, AppImage, Docker Desktop extension | Installers, Docker image, a plain manifest | Medium |
| 20 | Community | SIG UI project, 7.5k stars, monthly meetings | New project | n/a |

## Details

### 1. Logs and terminals get the room they need

Headlamp opens logs, terminals and the editor as an "activity" that takes the full content area
by default (`location: 'full'` in `LogsButton.tsx` and `pod/Details.tsx`). An activity can be
moved to the left, right, top or bottom half, minimized to a task bar at the bottom and brought
back, and several can stay open at once, so a log keeps streaming while you look at something
else.

Kubri shows logs and the shell as a tab of the details panel, a narrow column beside the list.
<kbd>f</kbd> maximizes the panel, but nothing says so and the width cannot be dragged. Long log
lines are cut off or wrap into an unreadable column.

**Adopt:** open logs and shells full width by default, or in a bottom panel that takes half the
height and can be resized. Let a log or shell keep running in a task bar when you navigate away,
and allow more than one. (Being worked on separately.)

### 2. The log viewer itself

Headlamp's log viewer (xterm.js based, `common/LogViewer.tsx`) has, beyond what Kubri has:

- a severity filter that recognizes error, warning, info and debug lines (`logSeverityFilter.ts`);
- "Prettify", which formats JSON log lines;
- for a workload, a pod picker with "All Pods", next to the container picker;
- search with next and previous match and a match count, instead of filtering lines away;
- copy of exactly what is shown, and download with a timestamped file name.

Kubri already has a regex filter, export, timestamps, wrap, previous logs, k9s time ranges and
merged logs of a workload. **Adopt:** a severity filter, JSON prettify and a pod picker for
workload logs. Keep the filter, and add "find" with match navigation beside it.

### 3. Several clusters at once

Headlamp's cluster chooser takes several clusters at once. Lists then show a Cluster column and
merge objects from all selected clusters, and the home page shows all clusters with their status.
Kubri works on one context at a time and switches with `:ctx`.

**Adopt:** a home page with every context and its health, and an opt-in multi-cluster mode for
lists. The core already merges kubeconfigs; the watches would need a cluster key.

### 4. Managing clusters from the UI

Headlamp can load a kubeconfig by drag and drop or file picker and pick which contexts to add,
rename a cluster, give it an accent color and icon (so production looks different), set a
default namespace, restrict the namespaces shown with a list or label selector, and remove a
cluster. Kubri reads `KUBECONFIG` and `~/.kube/config` and nothing else.

**Adopt:** per-context color and display name first, since telling production apart is a safety
feature. Loading a kubeconfig file from the UI matters most for the web app.

### 5. Creating resources

Headlamp has a "Create" button in the sidebar that opens an editor for new YAML, can upload a
file or load one from a URL, and has guided forms for Pods, Deployments, StatefulSets,
DaemonSets, Jobs, CronJobs, Services and PersistentVolumeClaims. Before it applies a change it can show a dry-run preview of what the API
server would store.

Kubri can only edit existing objects. k9s has no create either, but a GUI is where people expect
it. **Adopt:** "New from YAML" with a template per kind, applied with server-side dry run first
and the diff shown. Forms can come later.

### 6. A UI that knows what you may do

Headlamp checks permissions before it shows an action (`AuthVisible.tsx`, `allowedNamespaces`):
delete, edit, scale and the like are hidden or disabled when the user lacks the verb, and lists
fall back to the namespaces the user may read when cluster-wide listing is forbidden.

Kubri shows every action and reports the API error afterwards. Kubri already runs
`SelfSubjectRulesReview` for the RBAC view (`crates/kubri-core/src/rbac.rs`), so the data is
there. **Adopt:** disable actions with a tooltip that names the missing permission, and fall back
to readable namespaces in lists.

### 7. Sign-in for the web app

Headlamp in a cluster supports OIDC directly, so every user acts with their own identity and
RBAC, with guides for Keycloak, Dex with OAuth2 Proxy, Azure Entra ID, EKS with Cognito,
OpenUnison and Identity-Aware Proxy, plus service account tokens and basic auth. It ships a Helm
chart for this.

Kubri's web mode has one shared password and acts with one service account or kubeconfig for
everybody, or trusts an auth proxy (`KUBRI_NO_AUTH`). **Adopt:** OIDC sign-in that passes the
user's token to the API server, so a team gets per-user permissions and audit logs. Together
with item 6, this is what makes Kubri usable as a shared team tool.

### 8. Plugins that change the UI

Headlamp plugins are JavaScript bundles that can add sidebar entries, routes, detail sections,
table columns, header actions, map nodes and relationships, themes and settings. The desktop app
has a plugin catalog backed by Artifact Hub, and in-cluster installs get a plugin manager sidecar.

Kubri supports k9s plugins, which run shell commands on a resource. That keeps k9s
compatibility, but nobody can add a view for their own CRDs. **Adopt (later):** a small extension
API for detail sections and columns per CRD would cover most needs without a full plugin system.

### 9. Ecosystem integrations

The official plugins cover Prometheus (charts in detail views), Flux, Argo CD, cert-manager,
KEDA, Karpenter, Kyverno, Velero, OpenCost, Knative, Cluster API, Kueue, Volcano, Kubeflow,
Strimzi, kro, minikube, Backstage and more, plus an AI assistant. Kubri knows Helm and shows any
CRD generically.

**Adopt:** Prometheus charts for pods and nodes are the most useful single step, since Kubri's
Pulses already show trends from metrics-server only. After that, Flux and Argo CD status
(sync, health, reconcile action) cover the most users.

### 10. Installing Helm charts

Headlamp's App Catalog plugin browses Artifact Hub and installs charts. Kubri shows releases,
values, manifests and history and can roll back and uninstall, but cannot install or upgrade.
**Adopt:** upgrade with edited values first; a catalog is less important.

### 11. Debugging

Headlamp can start an ephemeral debug container in a pod, aimed at a chosen container, with a
configurable image, and open a node shell through a privileged pod. Kubri can only shell into or
attach to containers that are already there, which fails for distroless images. **Adopt:**
`kubectl debug` for pods (ephemeral container) and nodes; k9s has a node shell too, behind a
feature gate.

### 12. Search

Headlamp has a global search box for every object and page, with recent items, and an advanced
search that runs a query expression (for example on `status.phase`) over any set of kinds and
clusters, with saved searches. Kubri has the command palette for kinds and per-list filters.
**Adopt:** let the palette find objects by name across kinds, and later field queries.

### 13. Resource map

Headlamp's map shows a namespace or the whole cluster as a zoomable graph (Deployments,
ReplicaSets, Pods, Services, Ingresses, ConfigMaps, nodes), grouped by namespace, node or
instance, with health colors and details on click. Kubri's XRay is a tree per kind, which reads
well but does not show how Services and Ingresses tie in. **Adopt (later):** a graph mode for XRay.

### 14. Projects

Headlamp "projects" group an application's resources across namespaces and clusters by label,
with tabs for overview, resources, access and map. Nothing comparable exists in Kubri or k9s.
Low priority.

### 15. Editor help

Headlamp's editor (Monaco) shows the API documentation of the kind beside the YAML, keeps
unsaved changes, and warns inline when the object changed on the server while you edit. Kubri's
CodeMirror editor guards unsaved changes but shows no field docs and only finds a conflict when
saving. **Adopt:** the `explain` docs beside the editor and a "changed meanwhile" notice.

### 16. Tables

Headlamp lets users pick columns, set rows per page, copy cells, and offers bulk delete, restart
and scale. Kubri shows the `kubectl get` columns plus the wide view and has bulk actions through
multi-select. **Adopt:** hide and show columns, remembered per kind.

### 17. Languages

Headlamp is translated into 26 locales, German included. Kubri is English only. Worth doing once
the UI text settles; it needs a string catalog first.

### 18. Accessibility and theming

Headlamp remembers the zoom level, has layouts for high zoom, a theme picker with live previews,
and contrast fixes for custom themes. Kubri has k9s skins plus light and dark. Kubri's own a11y work
(changeset 8 of the redesign) covers keyboard and labels; zoom and contrast checks are open.

### 19. Distribution

Headlamp ships a Helm chart for in-cluster installs, Flatpak, AppImage, Homebrew and Windows
package registries, and a Docker Desktop extension. Its desktop builds are unsigned too. Kubri
has installers per platform, a Docker image and a plain manifest. **Adopt:** a Helm chart and a
Homebrew cask are cheap and expected; Flatpak and winget next.

### 20. Community

Headlamp is a Kubernetes SIG UI project with monthly meetings, a Slack channel and many
contributors. This is not a feature to copy, but it explains why its plugin ecosystem grows.

## Where Kubri is ahead

To keep the list honest, these are things Kubri does that Headlamp does not:

- the k9s keyboard model, command palette, aliases, hotkeys, skins and plugins, read from the
  user's k9s config;
- Pulses, a sanitizer with a grade, RBAC "who can do what" matrices, HTTP benchmarks over
  port-forwards, and node drain with the checks of `kubectl drain`;
- a "likely problem" summary for failing pods, with events and previous logs beside the list;
- kubeconfig loading that matches `kubectl` exactly, including exec plugins and odd user entries;
- a small native app (Rust and Tauri) instead of Electron, with a read-only mode.

## Suggested order

1. Logs and shells full size or in a resizable bottom panel (item 1, in progress), then the log
   viewer additions (item 2).
2. Permission-aware actions (6) and OIDC sign-in for the web app (7).
3. Create from YAML with dry-run diff (5) and `kubectl debug` (11).
4. Cluster colors and names, a cluster home page (4, 3).
5. Prometheus charts (9), Helm upgrade (10), column chooser (16), Helm chart for Kubri (19).
6. Later: multi-cluster lists, resource graph, extension API, translations.
