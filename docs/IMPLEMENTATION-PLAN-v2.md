# Kubri implementation plan v2: beyond Headlamp

Date: 29 October 2026. Starting point: Kubri 0.2.2, with the full-size log and shell workspace
(PR #22) already merged. Status: agreed with the project owner, nothing implemented yet.

This plan turns [Where Headlamp does better than Kubri](HEADLAMP-COMPARISON.md) (items **H1** to
**H20**) and [Ideas: going beyond Headlamp](IDEAS.md) (ideas **I1** to **I19**) into phases with
one release each, ending at 1.0. The older [redesign plan](design-review/IMPLEMENTATION-PLAN.md)
is done and stays as the record of that work.

## Decisions

These were settled with the project owner before writing the plan:

- **Kubri is a single-user app.** It runs on the user's machine, as the desktop app or as the
  local web server. It will not be offered as a shared team service inside a cluster. So OIDC
  sign-in, a team audit trail, impersonation for admins and a Helm chart for Kubri itself are out
  of scope (H7, I5, part of I2 and H19).
- **One release per phase**: 0.3 to 0.9, then 1.0.
- **Several clusters**: a fleet home page with every context and a compare view are enough. Lists
  that merge objects from several clusters are not planned (part of H3).
- **AI**: Kubri becomes an MCP server early, and gets an "Explain" action that uses the user's own
  model API key. No key, no AI: nothing is sent anywhere by default.
- **Extensions**: k9s plugins plus declarative YAML views for CRDs. No JavaScript or WebAssembly
  plugin system (H8, I9).
- **Integrations**: Prometheus, Grafana, Loki, Flux, Argo CD and cert-manager.
- **Language**: English only (H17 is out).
- **No code signing.** Distribution channels that work with unsigned builds only.
- **No command-line tool and no local cluster creation** (I16, I18). k9s and `kubectl` already
  cover the terminal; Kubri's strength is its interface.

## Principles for every phase

- **The core does the work.** New features are computed in `kubri-core` (Rust), exposed through
  `kubri-server/src/api.rs` and the Tauri commands in `ui/src-tauri/src/lib.rs`, and reach the UI
  through `ui/src/lib/api/{http,tauri}.ts`. Desktop and web stay at parity.
- **Every action stays clickable and keyboard-driven.** New actions go into `objectActions` in
  `ui/src/lib/objects.ts`, so they appear in the action bar, row menu, palette and footer hints at
  once. k9s keys keep their meaning.
- **Read-only mode and permissions are enforced in the core**, not only hidden in the UI. This
  matters most for the MCP server.
- **Tested against a real cluster** (k3s) for anything that
  talks to the API server, plus unit tests in Rust and Vitest.
- Each phase ends with updated docs, README screenshots where the UI changed, and a release.

## Overview

| Release | Theme | Main items | Effort |
| --- | --- | --- | --- |
| 0.3 ✅ | Work safely | Context colors and guard rails, permission-aware actions, copy as kubectl, dry-run diffs, editor help | 3 to 4 weeks |
| 0.4 ✅ | AI | MCP server, Explain action | 2 to 3 weeks |
| 0.5 ✅ | Logs and debugging | Log workspace tabs, structured logs, debug containers, node shell, fixes from the likely problem | 4 to 5 weeks |
| 0.6 ✅ | Create and change | Create from YAML, clone, `:new`, Helm install and upgrade, columns, port-forward favorites | 3 to 4 weeks |
| 0.7 ✅ | Integrations | Prometheus charts, Grafana, Loki, Flux, Argo CD, cert-manager, right-sizing | 5 to 6 weeks |
| 0.8 ✅ | Overview | Fleet home, cluster compare, clusters from the UI, global search, XRay graph | 4 to 5 weeks |
| 0.9 ✅ | Memory | What changed?, snapshots, declarative CRD views | 4 to 5 weeks |
| 1.0 ✅ | Finish | Speed numbers, zoom and themes, distribution channels, stabilization | 2 to 3 weeks |

The order puts safety first because the MCP server in 0.4 relies on it: an AI agent must hit the
same read-only and permission checks as a person. AI comes early as decided; the log and debug
work follows because PR #22 already solved the most visible log problem.

## 0.3: Work safely

Goal: you always know which cluster you are on, what you may do there, and what an action will
change before it runs.

### Context colors, names and guard rails (H4 part, I4 part)

- Per-context settings: display name, accent color, an "is production" flag, a default namespace.
  Stored in Kubri's own settings file next to the k9s config, never written into the kubeconfig.
- The accent color tints the header and context switcher; production contexts get a banner.
- Destructive actions on production contexts ask to type the object or context name. Bulk deletes
  always show the full list (the dialog from redesign changeset 2 already does).
- Production contexts can start read-only with "Unlock for 15 minutes". This builds on the existing
  `readOnly` from the k9s config (`crates/kubri-core/src/k9s.rs`); the unlock is temporary and per
  session.

### Permissions that explain themselves (H6, I2 without impersonation)

- The core computes allowed verbs per kind and namespace from `SelfSubjectRulesReview`
  (`crates/kubri-core/src/rbac.rs`, the `/permissions` route already exists), cached per context
  and refreshed on context switch and on a 403.
- Actions the user may not run are shown disabled with a tooltip: "Needs `delete` on `pods` in
  `shop`". The palette and keys give the same message instead of failing on the API call.
- When listing all namespaces is forbidden, lists fall back to the namespaces the user may read,
  found on their own.
- "Who may do this?" in the tooltip opens the RBAC view filtered to that verb and resource, and
  offers a ready-made Role and RoleBinding as YAML to send to an admin.

### Copy as kubectl (I3 part)

- Every action in `objectActions` can produce the equivalent `kubectl` command for the current
  context and namespace (`kubectl --context x -n y scale deploy/web --replicas=3`).
- Shown in the confirmation dialog and as "Copy as kubectl" in the row menu. Pure function in the
  UI, unit tested per action.

### Dry-run diffs (I3 part)

- YAML edits, scale, image changes, label and annotation changes and restarts run a server-side
  dry run first (`dryRun=All`) and show the diff between live and resulting object.
- A setting decides which actions show the diff; YAML edits always do, scale and restart can skip.
- The diff ignores `managedFields`, `resourceVersion` and status, like `kubectl diff`.

### Editor help (H15)

- `kubectl explain` style field docs beside the YAML editor, from the OpenAPI v3 schema the API
  server publishes, following the cursor position in `YamlEditor.svelte`.
- Schema-based completion and validation of field names in CodeMirror.
- "Changed on the server meanwhile" notice while editing, from the existing watch, with a
  three-way view (yours, theirs, original) instead of a failed save.

**Acceptance:** on a context marked production, the header shows its color and a destructive
action needs the typed name; with a user that lacks `delete` on pods, Delete is disabled and names
the missing permission; every action can be copied as a working `kubectl` command; editing a
Deployment shows the dry-run diff before saving.

## 0.4: AI

Goal: AI agents can use Kubri as a safe window into the cluster, and people can ask for an
explanation of what they see.

### Kubri as an MCP server (I15)

- A new crate `kubri-mcp` on top of `kubri-core`, speaking MCP over stdio (for local agents such
  as Claude Desktop, Claude Code or IDEs) and over streamable HTTP on localhost from the desktop
  app and `kubri-server`.
- Tools: list contexts, list objects with the same table columns as the UI, get YAML, describe,
  events, logs (bounded), likely problem for a pod, sanitizer report, XRay tree, RBAC check,
  Pulses. Later phases add What changed (0.9) and metrics (0.7) as tools.
- Read-only by default. Write tools (scale, restart, delete, apply) exist only when the user turns
  them on per context, and each call asks for confirmation in the Kubri window when it is open.
  Production contexts from 0.3 never get write tools.
- Secrets are redacted unless the user allows them. Results are size-limited and paginated.
- A settings page shows the config snippet to paste into an MCP client, and a log of tool calls.

### Explain (I15 part)

- "Explain" on a pod, an event, a log selection, a sanitizer finding or a failed action. It sends
  the relevant context (object, events, likely problem, log excerpt) to a model the user chooses
  and shows the answer beside it.
- Provider and key in settings, stored in the OS keychain on desktop. Anthropic and any
  OpenAI-compatible endpoint (which also covers local models such as Ollama).
- Before sending, the user sees exactly what will be sent; Secrets and tokens are stripped.

**Acceptance:** an MCP client connected to Kubri can answer "why is pod X failing" using Kubri's
tools; with read-only mode on, no write tool is listed; Explain on a crash-looping pod shows what
it will send and returns an answer with the user's key.

## 0.5: Logs and debugging

Goal: from "this pod is broken" to the cause and a fix without leaving Kubri.

### Log workspace (H1 rest, H2, I1)

PR #22 gave logs and shells the whole workspace. Still missing:

- Tabs for several logs and shells that keep running while you browse, minimized into a task bar;
  docking at the bottom or right, resizable.
- Find with next and previous match and a match count, beside the existing filter.
- Severity filter (error, warning, info, debug) recognized from common formats.
- Structured logs: JSON and logfmt lines as a table with chosen fields as columns, filters such as
  `level=error status>=500`, pretty-print one line on click.
- Workload logs with a pod picker ("All pods" or one), a color per pod, new pods joining during a
  rollout, restarts and rollouts as markers.
- A log-rate histogram on the time axis with events and restarts (reusing `Histogram.svelte`);
  click a spike to jump there.
- Folded stack traces and repeated lines ("× 312"); copy exactly what is shown.
- Two pods side by side.

Parsing and filtering run in `crates/kubri-core/src/logs.rs`, so large streams stay fast.

### Debug containers and node shell (H11, I6 part)

- `kubectl debug` for pods: an ephemeral container aimed at a chosen container, sharing its process
  namespace, with a configurable image (default `busybox`, remembered).
- Debug copy of a pod with a changed command (`sleep infinity`) for crash loops.
- Node shell through a privileged pod on the node, removed when the shell closes, behind a clear
  warning and disabled in read-only mode.

### Fixes from the likely problem (I6)

The "likely problem" summary (`PodSummary.svelte`) gets one action per diagnosis:

- crash loop: previous logs and the debug copy;
- OOMKilled: memory use against the limit and a prefilled patch to raise it, with the dry-run diff;
- ImagePullBackOff: the image, the pull secret and the registry used;
- Pending: per node, why the scheduler refused (taints, resources, affinity);
- a Service without endpoints: selector against pod labels, readiness and ports, naming the part
  that does not match.

**Acceptance:** three logs stay open in tabs while browsing; a JSON log can be filtered by field;
a distroless pod can be debugged with an ephemeral container; an OOMKilled pod offers a working
patch.

## 0.6: Create and change

Goal: everything `kubectl apply` and `helm upgrade` do, with a preview.

### Create resources (H5, I3 part)

- "New" in the sidebar and `:new` in the palette: an editor with a template per kind (built in for
  the common kinds, generated from the schema for others and for CRDs), or YAML from a file, a URL
  or the clipboard. Multi-document YAML is applied in dependency order.
- Server-side dry run and diff before applying, using 0.3's diff view.
- Short forms in the palette like `kubectl create`: `:new deploy web --image nginx --replicas 3`.
- Guided forms for Deployment, Service, Job, CronJob, ConfigMap and Secret, which write the same
  YAML the editor shows.

### Clone (I3 part)

- "Clone to…" another namespace or context, with server fields stripped and the diff shown.

### Helm (H10, I11)

- Upgrade a release with edited values, with a dry-run manifest diff.
- Install a chart from a repository, an OCI registry or Artifact Hub search, with its default
  values in the editor.
- Diff values and manifests between two revisions before rolling back.
- Drift: where live objects differ from the release manifest.
- Like rollback today, install and upgrade use the `helm` CLI that the desktop app finds on PATH
  and the Docker image ships.

### Tables (H16)

- Column chooser per kind (hide, show, reorder, add a JSONPath column), remembered per kind.
- Copy a cell or a column; bulk scale and restart beside bulk delete.

### Port-forwards that stay (I17)

- Favorite forwards per context that start with the app, reconnect after a pod restart and open in
  the browser with one click.

**Acceptance:** a Deployment created from a template shows its dry-run diff and appears in the
list; a Helm release can be upgraded with changed values after reviewing the diff; a hidden column
stays hidden after a restart; a favorite forward survives deleting its pod.

## 0.7: Integrations

Goal: the tools most clusters run show up in Kubri without setup. Each is detected automatically
(by API group, Service or CRD) and can be pointed elsewhere in settings.

### Prometheus (H9, I10)

- Found through the API server service proxy (`crates/kubri-core/src/proxy.rs`) or a URL in
  settings.
- Charts in the details panel for pods, workloads and nodes: CPU, memory, restarts, network, and
  request rate where the usual metrics exist. Time ranges, and before/after markers for rollouts.
- Pulses gain history from Prometheus when it is there.

### Grafana

- When Grafana is found, links from objects to matching dashboards (by namespace, workload and pod
  variables), and a list of dashboards per namespace. Optional API token in settings.

### Loki

- Log history beyond the pod's lifetime: the log workspace from 0.5 gets "Loki" as a source, with a
  LogQL query prefilled from the object's labels, the same filters and the same histogram.

### Flux and Argo CD (H9, I10)

- Status columns and details: sync state, health, last reconcile, source revision, errors.
- Actions: reconcile or sync, suspend and resume.
- A warning before editing an object that GitOps manages ("managed by Argo CD app `shop`; changes
  will be reverted").
- XRay links from an Application or Kustomization to the objects it manages.

### cert-manager (H9, I10)

- Certificates with expiry and readiness, soon-to-expire certificates in Pulses and the sanitizer,
  "renew now".

### Right-sizing (I13)

- Workloads that request much more than they use, or run close to their limit, from
  metrics-server and from Prometheus history when available, with a suggested request and a
  prefilled patch through the dry-run diff.

**Acceptance:** on a cluster with kube-prometheus-stack, Loki, Flux or Argo CD and cert-manager,
each integration appears without configuration; a pod's CPU chart, its Loki history and its
Grafana dashboard link all work.

## 0.8: Overview

Goal: see every cluster at once and find anything in it.

### Fleet home and compare (H3 as decided, I4)

- A home page with every context: reachability, version, node and pod health from Pulses, the
  color and name from 0.3. Unreachable or unhealthy clusters stand out. Contexts are checked in
  parallel with a timeout and never block the UI.
- Compare an object across contexts or namespaces: the same Deployment in staging and production
  as a diff of spec, image tags and config.

### Clusters from the UI (H4 rest)

- Load a kubeconfig file by drag and drop or file picker, choose which contexts to add, and remove
  them again. Kubri keeps these in its own list; the user's kubeconfig files are never changed.
- Restrict the namespaces shown per context by a list or a label selector.

### Global search (H12, I8)

- The palette finds objects by name across all kinds and, on request, all contexts, with recent
  items. The core searches its watch caches, so it is fast on large clusters.
- The k9s filter grammar (`/regex`, `-l app=web`, `-f fuzzy`) plus field queries such as
  `status.phase!=Running`, over chosen kinds.
- Saved searches become entries in the sidebar. With a label selector across namespaces, a saved
  search also covers Headlamp's "projects" (H14): one application's resources in one view.

### XRay graph and blast radius (H13, I7)

- A graph mode for XRay: a namespace as a zoomable graph of workloads, pods, Services, Ingresses,
  ConfigMaps, Secrets and nodes, grouped by namespace, node or application label, colored by
  health.
- The request path Ingress → Service → EndpointSlice → Pod with the broken link highlighted.
- Blast radius in the delete dialog, from the reference finder (`references.rs`): "Deleting this
  Secret breaks 3 Deployments and 1 Ingress TLS".

**Acceptance:** the home page shows ten contexts within a few seconds even when some are down;
searching `web` finds Deployments, Services and pods of that name; deleting a used Secret lists
what breaks.

## 0.9: Memory

Goal: Kubri remembers what happened and can hand it over.

### What changed? (I12)

- While Kubri is running, the core records changes from its watches into a local SQLite database:
  rollouts, spec and image changes, ConfigMap and Secret edits (keys only, never values), scaling,
  node events, warning events. Size and age limits in settings.
- A timeline per cluster, namespace and object with filters, each entry opening its diff. "Last
  hour in this namespace" from the palette and as an MCP tool.
- Markers from the timeline in Prometheus charts and the log histogram.
- Honest about gaps: shown only for the time Kubri was watching, with the gaps visible.

### Snapshots (I14)

- Save a namespace or cluster (objects without Secrets' data, events, recent log lines, Pulses,
  sanitizer report) to one file, and open it in Kubri offline with the normal UI in read-only
  mode, clearly labeled as a snapshot. For "send me what you see" between people.

### Declarative CRD views (H8, I9)

- YAML files in Kubri's config folder describe a CRD: columns as JSONPath, health rules, actions
  as commands (like k9s plugins), detail sections, relationships for XRay.
- Kubri ships views for common CRDs (the integrations from 0.7 are written this way where
  possible). Loaded in the desktop app and the local web server alike, since they run no code.

**Acceptance:** after a rollout, What changed shows it with the image diff; a snapshot opens on a
machine without cluster access; a view file adds columns and a health state to a CRD list.

## 1.0: Finish

### Proof of speed (I19)

- A benchmark job in CI: startup time, memory and time to list 10,000 pods on k3s, for Kubri and
  Headlamp, with results in the docs and a threshold that fails CI on regressions.

### Accessibility and themes (H18)

- Zoom level remembered, layouts checked at 200% zoom, a theme picker with previews, and a contrast
  check that warns about unreadable k9s skins.

### Distribution (H19, unsigned)

- A Homebrew cask in a `snaketech-tu/homebrew-kubri` tap (the cask removes the quarantine flag,
  since the macOS build is not signed), winget manifests, an AUR package and Flathub (Flatpak). The
  release workflow opens the update PRs for each.
- An in-app notice when a new release is out, with a link to the release page.
- The Docker image and the plain manifest stay as they are for people who want the web mode in a
  container, without new in-cluster features.

### Stabilization

- A full review pass like PR #8 and PR #13, against k3s, fixing what it finds.
- Docs: a user guide per area, updated README and screenshots, migration notes from k9s.

## Coverage

Every item of both documents, and where it lands.

| Item | Where |
| --- | --- |
| H1 Logs and terminals full size | Done in PR #22; tabs and docking in 0.5 |
| H2 Log viewer | 0.5 |
| H3 Several clusters | Fleet home in 0.8; merged lists not planned |
| H4 Clusters from the UI | Colors and names in 0.3, loading files in 0.8 |
| H5 Create resources | 0.6 |
| H6 Permission-aware UI | 0.3 |
| H7 OIDC sign-in | Out of scope (single-user app) |
| H8 Plugins | Declarative CRD views in 0.9; no code plugins |
| H9 Integrations | 0.7 (Prometheus, Grafana, Loki, Flux, Argo CD, cert-manager) |
| H10 Install Helm charts | 0.6 |
| H11 Debugging | 0.5 |
| H12 Search | 0.8 |
| H13 Resource map | 0.8 |
| H14 Projects | Saved searches with label selectors in 0.8 |
| H15 Editor help | 0.3 |
| H16 Tables | 0.6 |
| H17 Languages | Out of scope (English only) |
| H18 Accessibility and theming | 1.0 |
| H19 Distribution | 1.0 (no Helm chart, no signing) |
| H20 Community | Not a feature |
| I1 Log workspace | 0.5 |
| I2 Permissions that explain themselves | 0.3; impersonation out of scope |
| I3 Create and edit with a diff | Diffs and copy as kubectl in 0.3, create and clone in 0.6 |
| I4 Multi-cluster that compares | Guard rails in 0.3, fleet home and compare in 0.8 |
| I5 Team sign-in with audit | Out of scope; the MCP call log (0.4) and What changed (0.9) cover the single-user need |
| I6 Debugging that finishes the job | 0.5 |
| I7 XRay graph and blast radius | 0.8 |
| I8 Search with the k9s grammar | 0.8 |
| I9 Extensions without JavaScript | 0.9 |
| I10 Built-in integrations | 0.7 |
| I11 Helm drift and values diff | 0.6 |
| I12 What changed? | 0.9 |
| I13 Right-sizing | 0.7 |
| I14 Snapshots | 0.9 |
| I15 MCP server and Explain | 0.4 |
| I16 Command-line tool | Not planned |
| I17 Port-forwards that stay | 0.6 |
| I18 Local clusters | Not planned |
| I19 Proof of speed | 1.0 |

## Risks

- **Permission checks** differ between clusters (webhook authorizers, aggregated roles).
  `SelfSubjectRulesReview` can be incomplete; fall back to `SelfSubjectAccessReview` per action
  and never block an action only because the review was unsure.
- **MCP write tools** are the riskiest feature. Keep them off by default, behind confirmation, and
  enforce read-only and production rules in the core.
- **Integrations drift** with upstream versions (Argo CD and Flux CRDs change). Test against
  pinned versions in k3s and keep each integration small.
- **What changed** stores cluster data on disk. Keep Secret values out, document where the database
  lives, and offer "clear history".
- **Unsigned builds** cause warnings on macOS and Windows and may be refused by some package
  repositories; document the steps per platform.
