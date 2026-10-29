# Ideas: going beyond Headlamp

[Kubri and Headlamp](HEADLAMP-COMPARISON.md) lists what Headlamp does better today. This document
goes one step further: for each of those areas, how Kubri could do it better than Headlamp, and
which new ideas neither tool has yet. It builds on what Kubri already has: the k9s keyboard model,
a Rust core that does the work on the server side, the sanitizer, Pulses, XRay, the reference
finder, the "likely problem" summary and a read-only mode.

Written on 2026-10-29 after reading Headlamp's docs (`docs/` in its repository: architecture,
FAQ, in-cluster install, projects, local cluster, AI assistant, MCP and HolmesGPT support,
telemetry) and source at 0.46.0.

Effort: **S** is days, **M** is one or two weeks, **L** is more.

## What Headlamp's docs tell us

Headlamp's design leaves gaps that Kubri can use:

- **The browser does the work.** Headlamp's Go backend is a proxy; the React frontend lists,
  joins and filters every object itself. Its advanced search loads up to 10,000 items per kind
  by default and re-fetches them every 60 seconds. Kubri's core already watches and computes tables in Rust, so it
  can stay fast on clusters where a browser tab gives up.
- **Tokens live in the browser.** The FAQ says Headlamp "may store the token in the browser's
  local storage". Kubri's web mode keeps credentials on the server.
- **Narrow permissions are an afterthought.** By default Headlamp assumes the user can list all
  namespaces; otherwise the user must enter the allowed namespaces in the cluster settings by
  hand. Kubri can find them out with `SelfSubjectRulesReview` and access reviews, which the RBAC
  view already uses.
- **Smart features are plugins and need setup.** Prometheus charts, Flux, Argo CD, Helm
  install and the AI assistant are each a plugin; the AI assistant needs a provider key, and MCP
  servers work only in the desktop app. Kubri can build the few that matter into the core and
  have them work out of the box.
- **Mouse first.** Headlamp has a shortcut settings page, but its flows are built around
  dialogs and buttons. Kubri can be equally clickable and much faster by keyboard.
- **Electron.** Headlamp's latest release spent effort cutting startup memory by about 60 MiB.
  Kubri's Tauri app starts smaller and should say so with numbers.

## Doing Headlamp's features better

### 1. A log workspace, not a log dialog (M)

Headlamp opens logs in a full-size pane that can be split or minimized, with search, a severity
filter and JSON prettify. Kubri can go further:

- **Dock anywhere**: bottom, right or full, resizable, with tabs for several logs and shells
  that keep running while you browse. <kbd>l</kbd> opens, <kbd>f</kbd> toggles full size.
- **Structured logs as a table**: when lines are JSON or logfmt, show chosen fields as columns,
  filter with `level=error status>=500`, and pretty-print one line on click.
- **Follow a workload through rollouts**: new pods join the stream on their own, each pod gets a
  color, and restarts and rollouts appear as markers in the stream.
- **One timeline**: log rate per second as a small histogram (Kubri already draws them for
  benchmarks), with events and restarts on the same time axis. Click a spike to jump there.
- **Fold stack traces** and repeated lines ("× 312").
- **Compare two pods' logs side by side**, for "why does only this replica fail".

### 2. Permissions that explain themselves (S, then M)

Headlamp hides what you may not do. Kubri should show it disabled and say why: "Needs `delete`
on `pods` in `shop`". Then build on the RBAC view:

- "Who may do this?" lists the roles that grant it, and offers a RoleBinding to send to an admin.
- **View as** (impersonation): admins see the whole UI as a service account or user, to check a
  role before handing it out. Headlamp has nothing like it.
- Find the readable namespaces on its own instead of asking the user to type them.

### 3. Create and edit with a diff, every time (M)

Headlamp has forms and a dry-run preview for creating. Kubri can make every change reviewable:

- Every apply, edit, scale or image change shows a server-side dry-run diff first, like
  `kubectl diff`, with a setting to skip it for small actions.
- **Clone this object** into another namespace or cluster, with the diff.
- Create from the palette: `:new deploy web --image nginx --replicas 3`, like `kubectl create`.
- **Copy as kubectl**: every action can show the equivalent `kubectl` command. This teaches
  people Kubernetes, makes actions reproducible, and is cheap.

### 4. Multi-cluster that compares, not only lists (M to L)

Headlamp merges lists from several clusters. Kubri can add:

- A **fleet home**: Pulses for every context at once, so the unhealthy cluster stands out.
- **Compare across clusters**: the same Deployment in staging and production, as a diff of
  spec, image tags and config.
- **Context color and guard rails**: production contexts get a color, a banner and typed
  confirmation for destructive actions, or start read-only with a timed "unlock for 15 minutes".
  Kubri already has `readOnly` per context; this makes it friendlier.

### 5. Team sign-in with an audit trail (M)

Headlamp supports OIDC. Kubri should too, and then keep a record: who did what through Kubri,
when, on which cluster, with the diff. Shown in the app and exportable. With impersonation from
idea 2 this makes Kubri a safe shared tool, not just a viewer with a password.

### 6. Debugging that finishes the job (M)

Headlamp can start an ephemeral debug container and a node shell. Kubri can tie debugging to its
"likely problem" summary:

- One action per diagnosis: crash loop → previous logs and a debug copy of the pod with
  `sleep infinity` as command; OOMKilled → memory usage against the limit and a prefilled patch
  to raise it; ImagePullBackOff → the pull secret and registry used; Pending → why the scheduler
  refused, per node.
- `kubectl debug` for pods (ephemeral container, sharing the process namespace) and nodes.
- **Why does this Service have no endpoints?** Check the selector against pod labels, readiness
  and ports, and say which part does not match.

### 7. XRay as a graph with a blast radius (M)

Headlamp's map shows relationships. Kubri knows more: the reference finder already knows which
pods use a ConfigMap or Secret. Add a graph mode to XRay and a **blast radius** before deletes:
"Deleting this Secret breaks 3 Deployments and 1 Ingress TLS". Show the request path
Ingress → Service → EndpointSlice → Pod, with the broken link highlighted.

### 8. Search with the k9s grammar (S to M)

Headlamp has a global search and a query search with saved queries. Kubri's palette can search
object names across kinds and contexts with the filters people already know from k9s
(`/regex`, `-l app=web`, `-f fuzzy`), plus field queries like `status.phase!=Running`. Saved
searches become custom views in the sidebar.

### 9. Extensions without JavaScript (M)

Headlamp plugins are JavaScript bundles, powerful but a supply-chain risk and work to write.
Kubri can keep k9s plugins and add **declarative views** for CRDs in YAML: columns as JSONPath,
health rules, actions as commands, relationships for XRay. They are safe to load in the web app,
easy to share, and cover most of what CRD plugins do. A sandboxed WebAssembly API can come later
if needed.

### 10. Built-in integrations that need no setup (M each)

Rather than 30 plugins, build the few that most clusters have, detected automatically:

- **Prometheus**: find it through the API server proxy, show CPU, memory, restarts and request
  rates over time in the details panel, and compare before and after a rollout.
- **Flux and Argo CD**: show sync and health, reconcile and suspend, and warn before editing an
  object that GitOps will revert ("managed by Argo CD app `shop`").
- **cert-manager**: certificates that expire soon, in Pulses.

### 11. Helm with drift detection (M)

Beyond install and upgrade: show where the live objects differ from the release manifest, and
diff the values between two revisions before rolling back.

## New ideas neither tool has

### 12. "What changed?" (L)

Kubri watches the cluster anyway. Record changes (rollouts, spec and image changes, ConfigMap
edits, scaling, node events) into a local timeline, and answer the first question of every
incident: what changed in the last hour, in this namespace? Click a change to see its diff.

### 13. Right-sizing (S to M)

Kubri already shows usage against requests and limits. Turn it into advice: the workloads that
request far more than they use, the ones close to their limit, and a suggested request with a
prefilled patch. Headlamp needs the OpenCost plugin for less.

### 14. Cluster snapshots for support (M)

Save a namespace or cluster (objects, events, the last log lines, Pulses) to one file, with
Secrets left out, and open it in Kubri offline. This is the "send me what you see" that today
takes a dozen `kubectl` commands.

### 15. Kubri as an MCP server (M)

Headlamp is an MCP client in its desktop app. Kubri can be the server: expose its core
(lists, describe, likely problem, sanitizer, XRay, What changed) as MCP tools, with read-only
mode and RBAC enforced. AI agents then get the same safe, well-structured view of the cluster
that people get in the UI. An optional "Explain" button with the user's own model key can come
on top.

### 16. `kubri` on the command line and in CI (S to M)

The core is a Rust library, so a small CLI is cheap: `kubri lint --context prod --format sarif`
runs the sanitizer in CI, `kubri snapshot` makes the file from idea 14. It brings Kubri to
people who never open a GUI.

### 17. Port-forwards that stay (S)

Favorite forwards per context that reconnect after a pod restart, open in the browser with one
click, and start with the app. Benchmarks already sit on top.

### 18. Local clusters with kind and k3d (M)

Headlamp creates local clusters with Minikube only. Offer kind and k3d too, which most
developers use, and k3s in a container on Linux.

### 19. Proof of speed (S)

Publish numbers: startup time, memory and time to list 10,000 pods for Kubri and Headlamp, and
keep them in CI so they do not regress. It is Kubri's clearest advantage and costs little to show.

## Recommended picks

| Pick | Why | Effort |
| --- | --- | --- |
| Log workspace (1) | The most visible gap, and users already ask for it | M |
| Permissions that explain themselves (2) | Uses the RBAC code we have; Headlamp only hides buttons | S |
| Copy as kubectl and dry-run diffs (3) | Cheap, unique, builds trust | S to M |
| Context colors and guard rails (4) | Safety for production, cheap | S |
| Debug actions from the likely problem (6) | Turns Kubri's diagnosis into fixes | M |
| What changed? (12) | No other GUI has it, answers the first incident question | L |
| Kubri as an MCP server (15) | Makes Kubri the safe cluster interface for AI agents | M |

A good order: 1, 2, 4 and "Copy as kubectl" first (weeks), then 3, 6 and OIDC with audit (5),
then 12 and 15 as the features that set Kubri apart.
