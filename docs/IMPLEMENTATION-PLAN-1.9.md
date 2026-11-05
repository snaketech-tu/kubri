# Kubri implementation plan: 1.9

Date: 5 November 2026. Starting point: Kubri 1.8.0 (main at `9f7bb99`). Status: implemented
in pull requests #157 to #164, waiting for review. "As built" below lists where the result
differs from this plan.

1.9 makes everyday work smoother in four places:

- **Start where you left off** (**S1** to **S2**): Kubri connects to the context you used last,
  in the namespace and view you left.
- **Port-forwards that stay open** (**P1** to **P4**): users report connections that end too
  early or in one direction only. Find the causes against a real cluster and fix them.
- **Limits in the pod list** (**L1** to **L2**): next to CPU and memory use, the list shows what a
  pod may use at most.
- **Requests and limits made easy** (**R1** to **R4**): one dialog for a workload's requests and
  limits, with the room the nodes still have shown as a budget.

## As built

| Item | Pull request | Differences from the plan |
| --- | --- | --- |
| S1, S2 | #157 | The last context, the start setting and the views are kept in the app's local storage, not in the settings file, so each browser and the desktop app remember their own. The setting is **On start** in Settings. The namespace scope is not restored. |
| P1 to P4 | #159 | Both causes were confirmed against k3s: the client's half-close ended the connection at once, and idle connections dropped behind proxies. Kubri now runs its own forwarder loop: it keeps reading after the client is done until the pod closes or is quiet for 15 seconds, and pings every 20 seconds. The Port-forwards view says how the last connection ended. |
| L1, L2 | #158 | The request and limit columns need metrics-server, like the use columns. Sums per workload were left out. |
| R1 to R3 | #161 | The action is **Requests & limits…** on <kbd>Shift</kbd>+<kbd>Q</kbd>. Warnings about room do not block setting the values. Right-sizing's **Review…** fills in all suggestions of the workload. The MCP server does not use the budget yet. |
| R4 | #162 | Only "CPU free" and "Mem free" in the Nodes list. Quotas show in the dialog instead of the namespace details; the overview tile already opens the Nodes list; one review per workload replaces a multi-select in Right-sizing. |
| Added | #160 | The Integrations page layout: tools found, tools missing and the settings each get their own place, also at narrow widths. |
| Added | #163 | Label templates for logs: other label names for namespace, pod and container (such as `exported_namespace`), kept once and picked per context, used by Logs in Grafana and the Loki log history. |
| Added | #164 | Production namespaces: in a context that is not production, single namespaces get its care (banner, typed names, notes, no changes by AI agents). The automatic lock stays per context. |

## Decisions

- **Kubri does not write `current-context` into the kubeconfig.** Remembering the last context
  is Kubri's own setting. `kubectl` in other terminals keeps working as before. The local
  terminal already gets its own overlay with the context Kubri shows.
- **The kubeconfig's `current-context` still wins once.** When the kubeconfig's
  `current-context` changed since Kubri last started (someone ran `kubectl config use-context`),
  Kubri follows it, because that is the newer wish. Otherwise it opens the remembered context.
- **Port-forward fixes start with reproducing.** Every cause below is a hypothesis until a test
  against k3s shows it. The fix for a cause comes with the test that showed it.
- **Changing requests and limits stays a deliberate step.** No key changes a value without the
  dialog, because every change replaces the pods with a rolling update. Keys move values in
  steps inside the dialog, where the diff and the budget are visible.
- **One budget, computed in the core.** Free room per node and per namespace quota is computed
  once in `kubri-core` and used by the dialog, the Nodes list and the MCP server alike.

## Principles

- **Reuse what is there.** `ContextSettings` and the settings file (`settings.rs`), the view
  kept per context (plan 1.5, F3), the port-forward code (`portforward.rs`), pod metrics with
  requests and limits (`metrics.rs`), `set_resources` and the right-sizing advice
  (`rightsizing.rs`), free room per node in the cause for pending pods (`fixes.rs`) and the
  cluster totals of `pulse.rs`.
- **Every change gets a test.** Rust tests with `kubri_core::testing::client`, vitest tests for
  UI logic, and end-to-end tests in `scripts/e2e/` for what only shows against a cluster.
- **Desktop and web stay at parity.** In team mode, the last context is remembered per person.
- **k9s keys stay as they are.** New keys are checked against k9s 0.51 before they are taken.

## Overview

| Item | What | Where |
| --- | --- | --- |
| S1 | Remember the last context | `settings.rs`, `lib.rs` `contexts`, `state.svelte.ts` `load` |
| S2 | Remember namespace and view across restarts | `places.ts`, `state.svelte.ts` |
| P1 | Reproduce early and one-sided ends against k3s | `scripts/e2e/`, `portforward.rs` tests |
| P2 | Half-closed connections | `portforward.rs`, own forwarder loop |
| P3 | Idle connections | pings on the port-forward WebSocket |
| P4 | Say what happened | Port-forwards view, `PortForwardInfo` |
| L1 | Use against limit in the pod list | `ResourceView.svelte`, `columns.ts` |
| L2 | Limit and request columns | column chooser, `metrics.rs` |
| R1 | Action "Resources…" | `objects.ts`, `ActionDialog.svelte`, `rightsizing.rs` |
| R2 | Budget in the core | new `capacity.rs`, API, Tauri, `Backend` |
| R3 | Budget in the dialog | `ActionDialog.svelte` |
| R4 | Room in lists | Nodes list, namespace details, right-sizing |

Suggested PRs, each off `main`: S1+S2, P1 to P4 (one or two PRs, depending on what P1 finds),
L1+L2, R1, R2+R3, R4, then the release.

## Start where you left off

### S1. Remember the last context

Today Kubri connects to the kubeconfig's `current-context` on every start (`load` in
`ui/src/lib/state.svelte.ts` falls back to the context marked `isDefault`).

- The settings file gets `lastContext` and `lastKubeconfigContext` (the kubeconfig's
  `current-context` at the time). A successful `switch_context` updates both.
- On start, `contexts()` picks the context to connect to: the kubeconfig's `current-context`
  when it differs from `lastKubeconfigContext`, else `lastContext` when it still exists, else
  the kubeconfig's `current-context` as today.
- When the remembered context does not answer, Kubri shows the usual connecting and unreachable
  screen for it, with a button to use the kubeconfig's context instead. It does not silently
  fall back, so it is clear which cluster is shown.
- A snapshot opened at start and a context in the web app's address win over the remembered one.
- Team mode: remembered per signed-in person, in the person's browser (`localStorage`), not on
  the server.
- Setting under Settings, Clusters: "On start, connect to: the last context / the kubeconfig's
  current context". Default: the last context.
- Tests: core tests for the choice, including a deleted context and a changed `current-context`;
  vitest for `load`.

### S2. Remember namespace and view across restarts

Plan 1.5 (F3) keeps the page, kind and namespace per context for one session of the app.

- Save the view of each context (page, kind, namespace, scope; not filters or selection) to
  `localStorage`, for the ten most recent contexts.
- On start, the remembered context opens its remembered view, with the fallbacks of F3 when the
  kind or namespace is gone.
- Tests: vitest for saving, loading and the limit of ten.

## Port-forwards that stay open

Users report port-forwards that end early or work in one direction only. Today each local
connection opens its own stream through `Api::portforward` of kube-rs and copies with
`copy_bidirectional` (`connect` in `crates/kubri-core/src/portforward.rs`).

Likely causes, to confirm in P1:

1. **Half-close ends the whole connection.** When the local side stops sending (it shuts down
   its write half, as many clients do after a request), kube-rs's forwarder loop shuts down the
   local writer for that port and, once every port has closed, sends a WebSocket close. The
   pod's answer is then cut off, or never arrives. This matches "one-sided".
2. **Idle connections are dropped.** The port-forward WebSocket sends no pings. Load balancers
   and proxies in front of an API server (and the kubelet's streaming idle timeout) end idle
   connections after minutes. This matches "early", for databases and long-lived connections.
3. **An error on the error channel ends the copy at once,** without the bytes still on the way.
4. **Something outside the connection,** for example the forward stopping on context switch,
   on sleep and resume of the computer, or when the web session ends.

### P1. Reproduce against k3s

- An end-to-end test (`scripts/e2e/`) with a small test pod that offers: an echo server that
  answers after the client half-closes, a server that sends 50 MB, a connection idle for
  10 minutes (shortened with a proxy that has a short idle timeout in front of the API server),
  and a pod restart during a forward.
- Run the same cases with `kubectl port-forward` to know what users expect.
- Write down which cases fail and why, in the PR.

### P2. Half-closed connections

- If P1 confirms cause 1: Kubri runs its own small forwarder loop on the WebSocket from
  `Client::connect` (the same request that kube-rs builds), which passes a half-close to the
  pod and keeps the other direction open until the pod closes it too.
- The bytes still on the way are delivered before an error on the error channel ends the
  connection (cause 3).
- Tests: unit tests of the loop with an in-memory WebSocket; the P1 cases.

### P3. Idle connections

- If P1 confirms cause 2: a WebSocket ping every 30 seconds while a connection is open, and TCP
  keepalive on the local socket.
- Tests: the idle case of P1 behind a proxy with a 60-second idle timeout.

### P4. Say what happened

- `PortForwardInfo` gets the time and reason of the last ended connection ("the pod closed the
  connection", "the cluster ended the connection after it was idle", "the pod was replaced,
  moved to api-7c9…").
- The Port-forwards view shows it in the row, so a user can tell a Kubri problem from an app
  problem.
- Docs: `docs/SETTINGS.md`, where port-forwards are described, explains what each reason means.

## Limits in the pod list

Today the pod list shows CPU and memory use, and the share of request and limit as percentages.
The limit percentages only show in the wide view, and the list never shows the limit itself.

### L1. Use against limit

- The CPU and Mem cells show use and limit together, for example `120m / 500m` and
  `310Mi / 512Mi`, with a thin bar under the value that turns yellow and red at k9s's
  thresholds. Without a limit, the cell shows the use and "no limit" in its tooltip.
- The tooltip lists request, limit and use per container.
- Sorting by these columns stays by use; sorting by share stays on the percentage columns.
- The same for workloads where the list already sums pod metrics.
- Tests: vitest for the cell text and the bar, including pods with some containers without a
  limit (then the pod has no limit, as `metrics.rs` already counts it).

### L2. Limit and request columns

- Optional columns "CPU request", "CPU limit", "Memory request", "Memory limit" in the column
  chooser, for pods and workloads, sortable.
- They also work without metrics-server, since they come from the pod spec.

## Requests and limits made easy

Kubri can already set requests and limits (`set_resources` in `rightsizing.rs`, with
`ensure_writable` and `ensure_may("patch")`). The dialog is only reachable from right-sizing and
from the cause for pending pods, and it shows one value of one container.

### R1. Action "Resources…"

- A new action in `objectActions` (`ui/src/lib/objects.ts`), group Change, for Deployments,
  StatefulSets, DaemonSets and pods (a pod opens the dialog for the workload that owns it; a pod
  without an owner gets the action disabled with an explanation). It shows in the action bar,
  row menu, details panel, command palette and on a key.
- Key: <kbd>Shift</kbd>+<kbd>Q</kbd> (quota), after checking k9s 0.51. Command: `:resources`.
- One dialog for all containers: CPU and memory, request and limit, the current value, the use
  (95th percentile or peak, where known) and the right-sizing suggestion next to each field.
  "Use suggestions" fills them in.
- <kbd>↑</kbd> and <kbd>↓</kbd> in a field move one step: CPU in 50m steps up to 1 core, then in
  halves of a core; memory in powers of two (64Mi, 128Mi, 256Mi…). An empty limit means no
  limit. LimitRange defaults and bounds of the namespace show as placeholders and are checked.
- `set_resources` takes several containers, so one change is one patch and one rolling update.
- Diff, "Same as `kubectl set resources`", read-only, production confirmation and audit work as
  for the other changes.
- Tests: core test for the patch with several containers; vitest for the steps and the parsing
  of quantities.

### R2. Budget in the core

- A new core call, for a workload: per resource, allocatable and requested on the nodes the
  workload may run on (node selector, affinity and taints, as the cause for pending pods checks
  them in `fixes.rs`, moved into a shared function), the free room of each such node, and the
  namespace's ResourceQuota with what is used of it.
- Through every layer: core, route in `api.rs` (read only, no audit entry), Tauri command,
  `Backend`, `http.ts`, `tauri.ts`; and as an MCP tool, so agents can see the budget too.
- Without the right to list nodes, the call says so instead of giving wrong numbers.
- Tests: with `kubri_core::testing::client`, including taints, a node selector and a quota.

### R3. Budget in the dialog

Under each resource, a bar:

- what the matching nodes can give, what is requested, and the change of this dialog
  (new request minus old request, times replicas; for DaemonSets times the nodes it runs on),
  shown on the bar;
- the node with the most free room, and whether one pod with the new request fits on it;
- the namespace quota as a second bar, when there is one.

Warnings: yellow when more than 90 % would be requested, when the request is below the measured
use, or the limit below the peak. Red when no new pod would fit on any node, or fewer pods than
replicas fit; setting it then needs a confirmation that says the new pods would stay pending. A
note when the room only suffices without the extra pods of the rolling update (`maxSurge`).

### R4. Room in lists

- Nodes list: optional columns "CPU free" and "Memory free" (allocatable minus requests),
  sortable.
- Namespace details: requests of the namespace against its quota.
- Overview: the "requested" tile opens the Nodes list sorted by free room.
- Right-sizing: mark several rows, "Use suggestions" for all of them, one patch per workload,
  with the room it frees on top.

## Docs and release

- `docs/CLUSTERS.md` or `docs/SETTINGS.md`: the start setting (S1).
- `docs/SETTINGS.md`: the port-forward reasons of P4.
- `docs/VIEWS.md` or the matching page: the new columns (L1, L2, R4).
- A section on changing requests and limits (R1 to R3), and the new key in `docs/K9S.md` under
  Kubri's own keys.
- German texts for everything in `ui/src/lib/i18n/de.ts`.
- Release 1.9.0 as described in `docs/RELEASING.md`.

## Checks before release

- `pnpm check`, `pnpm test`, `cargo fmt`, `cargo clippy`, `cargo test`, and the end-to-end tests
  against k3s, including the new port-forward cases.
- Desktop app: restart lands in the last context and view; the dialog works by keyboard alone.
- Light and dark theme for the new cells, bars and dialog.
