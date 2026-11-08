# State machines in Kubri

Source audit: 2026-11-08, based on v1.4.1 (`baa394f`). This is a map of application-owned lifecycle and interaction
state across the Svelte UI, HTTP/Tauri adapters, Rust core, and MCP server. The state
names below describe the code's behavior; they are not all existing enum values.
Related component instances are grouped into families. Kubernetes controllers and
third-party library internals are outside this inventory. Static analysis cannot
prove that every runtime interleaving has been covered.

The lifecycle fixes were subsequently rebased and verified against v1.10.0
(`4987d33`). The broader inventory and follow-up findings describe the original
audit baseline; intervening features and fixes require a fresh audit before
treating those findings as current. On this Windows environment, unchanged
`4987d33` has 336 passing and 58 failing UI tests; the rebased changes have 356
passing tests and exactly the same 58 failures. All 15 MCP tests, UI type checking,
and the production build pass on the rebased code.

## Decision

Make lifecycle boundaries explicit incrementally, with tests for competing events.
Keep state local to its owner and preserve public API and wire formats. Use a small
union for mutually exclusive phases; keep independent choices (such as log wrap and
follow) independent. A single app-wide state machine would couple unrelated tools
and make changes harder for people and coding agents to reason about.

The immediate changes address terminal teardown and abandoned MCP approvals.
Neither change adds dependencies, changes cluster permissions, or changes protocol
payloads. UX improves because closed work stays closed; DX improves because the
transition rules are named and tested; AX improves through the same predictable
lifecycles, source map, and removal of obsolete approval requests.

## Inventory

Paths in the UI column are relative to `ui/src/lib/` unless linked elsewhere.
“Invariant” states the rule to preserve or verify when modifying that workflow,
not a claim that every existing implementation already enforces it.

| Machine / family | Owner and current representation | Transitions and invariant |
| --- | --- | --- |
| App bootstrap and cluster connection | [state.svelte.ts](../ui/src/lib/state.svelte.ts), [core lib.rs](../crates/kubri-core/src/lib.rs); `starting`, `connecting`, `cluster`, `error`, session locks | Loading contexts → connecting → discovery → connected/error; retry or switch. Resource data and permissions must belong to the committed context. |
| Browser authentication | [session.svelte.ts](../ui/src/lib/session.svelte.ts), `components/SignIn.svelte`, [auth.rs](../crates/kubri-server/src/auth.rs); checked/required/signedIn, recheck promise, cookies | Unknown → anonymous/authenticated; login, logout, expiry, 401 recheck. Desktop bypasses browser login. A stale check must not undo a newer login/logout. |
| Navigation and history | `state.svelte.ts`, `nav.ts`; page/resource/namespace/scope, history index | Browse → drill → back/forward; context switch resets history. Selection and details must resolve in the current context. |
| Unsaved edits and deferred navigation | [navigation.svelte.ts](../ui/src/lib/navigation.svelte.ts), `components/DraftDialog.svelte`; dirty/saving/pending callbacks | Clean → dirty → save/discard/cancel; failed save retains draft and destination. Only one accepted exit should run. |
| Inspector, tabs, and editor | `components/DetailsPanel.svelte`, `YamlEditor.svelte`, `threeway.ts`; object/error, editing/saving, server baseline, previews, opened tabs | Loading → viewing → editing → review/save or conflict resolution; polls and merges run alongside editing. Preserve local edits and reject results for old targets. |
| Dock and tool ownership | [dock.svelte.ts](../ui/src/lib/dock.svelte.ts), `components/Dock.svelte`, `DockPane.svelte`; sessions, active/beside/focused/last, minimized | Add/show/split/minimize/close; context changes close foreign sessions. Pane IDs must refer to live sessions and split panes must differ. |
| Table watch | [watch.svelte.ts](../ui/src/lib/watch.svelte.ts), [table.rs](../crates/kubri-core/src/table.rs), adapters; loading/hasLoaded/error/rows, stopped closure | Start → reset → upsert/delete → error → retry → stop. Retained rows distinguish stale data from an empty successful list. Old callbacks must not update the next watch. |
| Metrics polling | `watch.svelte.ts` LiveMetrics; polling/stopped/available/error/updated | Initial fetch → fresh → stale on error → recovery; stop cancels publication. Polls must not overlap; retained values must retain their freshness information. |
| Resource table interaction | `components/ResourceView.svelte`, `table.ts`, `select.ts`, `columns.ts` | Filtering/sorting → cursor/range selection → action/drill; list updates reconcile selection. Cursor, selected rows, and bulk targets must agree. |
| Permission query batching | [access.svelte.ts](../ui/src/lib/access.svelte.ts), [access.rs](../crates/kubri-core/src/access.rs); queued/asking/cached, undefined/null/boolean | Unknown → queued → checking → allowed/denied/unknown; refresh and context reset. Unknown is distinct from denial; the backend remains authoritative. |
| Read-only and temporary unlock | `state.svelte.ts`, core `lib.rs`, server `guard.rs`; Guard, unlockedUntil, request sequence, per-context expiry | Checking → read-only/locked/writable; unlock → timed writable → expiry/lock. Never transfer an unlock between contexts; core checks apply to every mutation. |
| Action dialog and bulk execution | `components/ActionDialog.svelte`, `actions.ts`, core `preview.rs`, `node.rs`, `objects.rs`; busy/error, confirmation, preview/previewed, drain report | Configure → validate → preview → confirm → execute → success/partial/error. Input changes invalidate review; destructive operations retain their confirmation and production checks. |
| Manifest creation/import/clone | [create.svelte.ts](../ui/src/lib/create.svelte.ts), `components/CreateView.svelte`, `create.ts`, core `apply.rs`; draft version/source, busy/results/applied/reviewedIn | Template/schema/file/URL/clipboard → draft → dry run → review → apply → per-item outcomes. Review must bind exact objects, namespace, source and destination contexts. |
| Debug sessions | `components/DebugDialog.svelte`, core `debug.rs`; ephemeral/copy, busy/error | Choose target/image → create debug session → open shell or error. Creation is a mutation; shell ownership follows the returned target. |
| Terminal and plugin terminal | [api/http.ts](../ui/src/lib/api/http.ts), [api/tauri.ts](../ui/src/lib/api/tauri.ts), `terminal.ts`, `components/TerminalView.svelte`, `PluginTerminal.svelte`, core `terminal.rs`, `plugin.rs` | Connecting → ready → exited; close is final. Display mounting, backend allocation, and transport readiness are separate events. A late open result still requires cleanup. **Lifecycle hardened in this change.** |
| Terminal input queue | [api/queue.ts](../ui/src/lib/api/queue.ts); waiting/ready/closed plus one in-flight write | Buffer → start → serialized writes → close. Closing drops buffered input, cannot undo a write already sent, and cannot restart. **Explicit lifecycle added in this change.** |
| Log subscription and playback | `components/LogView.svelte`, `logs.ts`, adapters, core `logs.rs`; connecting/streaming/ended, follow, previous, sources, stopped/ended | Subscribe → source discovery → lines → source end/global end; restart on source/options changes; Loki history is a separate request path. Filtering/folding/scroll follow must not alter stream ownership. |
| Port-forward ownership and recovery | `components/PortForwardsView.svelte`, `state.svelte.ts`, core `portforward.rs`; listener/tasks, target pod, error, favorites | Start/bind → accept → connect/re-resolve destination → stop. Forwards intentionally survive browsing/context changes; stopping must close owned connections. Favorites restart per context. |
| Benchmark job | `components/BenchmarksView.svelte`, core `bench.rs`; BenchState, stop channel, result counters | Start → running → finished/stopped; remove also stops. Workers must honor deadline/stop and count only completed attempts. This already has an explicit enum. |
| Dashboard and diagnostic polling | `components/PulsesView.svelte`, `XrayView.svelte`, `GraphView.svelte`, `SanitizerView.svelte`, `RightSizingView.svelte`, `MetricsPanel.svelte`, `RbacView.svelte` | Initial load → data/empty/error → refresh; scope changes start a new lifetime. Distinguish no results, no permission, missing integration, stale data, and failures. |
| Object-related asynchronous panels | `components/PodSummary.svelte`, `ServiceSummary.svelte`, `DashboardList.svelte`, `ManagedNote.svelte`, `RolloutHistory.svelte`, `ViewSummary.svelte` | Target → loading/checking → result/error → retry or action. Auxiliary data must stay bound to its object and context. |
| Fleet and kubeconfig import | `components/FleetView.svelte`, `AddContextsDialog.svelte`, core `fleet.rs`, `added.rs`, `kubeconfig.rs`; found/failed/checking, found/chosen/busy | Per-cluster checks → partial aggregate; import text → inspect → select → add; remove/reload contexts. One failed cluster must not hide successful clusters. |
| Search and saved searches | `components/SearchView.svelte`, `CommandPalette.svelte`, `search.ts`, core `search.rs`; typed/searched, busy/found/error, saved query | Type → search → partial results/error → navigate/save. Results must identify the query/context selection that produced them. |
| Cross-cluster comparison | `components/CompareDialog.svelte`, `compare.ts`; busy/error/shown | Choose target → fetch both → compare or error; change target → new comparison. Never label old data with a new destination. |
| Helm browsing/history/drift | `components/HelmView.svelte`, `HelmCompare.svelte`, core `helm.rs`; selected release/revision/tab, details/error, drift | List → detail/history → revision/compare/drift → rollback/uninstall or upgrade. Independent requests must remain keyed to release and revision. |
| Helm install/upgrade | `components/HelmChangeView.svelte`; chart source, searching, busy string, review key | Chart search/defaults → values draft → render → review → apply/error. Bind review to captured inputs, including the context, before starting async rendering. |
| Settings, skins, and integrations | `state.svelte.ts`, `components/ConfigView.svelte`, `ContextSettingsForm.svelte`, `IntegrationsView.svelte`, `ExplainSettings.svelte`, core `settings.rs`, `integrations.rs`, `grafana.rs` | Load/cache → edit → save → rediscover/apply or failure; skin request sequence rejects obsolete completions. Cache scope and persisted settings must match the active context. |
| Snapshots | `components/SnapshotDialog.svelte`, `state.svelte.ts`, core `snapshot.rs` | Live → capture/export; import → decode → offline snapshot → reconnect live. Offline mode remains read-only and uses snapshot time. |
| Change recording and history | `components/ChangesView.svelte`, core `recorder.rs`, `history.rs`; recording lease/task, heartbeat, pages/more/error | Lease acquisition → initial reconciliation → watches → heartbeat/recovery → drop; query → pagination → detail. Avoid double recording and don't claim unwatched intervals as covered. |
| Agent server and token lifecycle | `components/AgentSettings.svelte`, desktop/server `agents.rs`, MCP `http.rs`, `stdio.rs`, `lib.rs` | Disabled → listening → stopped/reconfigured; token rotation invalidates old credentials; MCP initialize → tools. Stdio never offers mutation tools. |
| Agent approval | [hub.rs](../crates/kubri-mcp/src/hub.rs), `components/AgentRequests.svelte`, MCP `tools.rs`; pending vector + oneshot + window heartbeat + timeout | Request → guard/window checks → waiting → allowed/refused/timed out/canceled → result. One answer consumes a request; allow does not bypass core permissions. **Canceled receivers are pruned before listing or answering.** |
| Explain consent | `components/ExplainPanel.svelte`, `explain.ts`, core/desktop `explain.rs`; gathering/preview/asking/answered, open/error | Gather → inspect prompt → explicit send → answer/error → close. Collecting information must not automatically send it; late answers cannot reopen a dismissed panel. |
| Release check and update | `releases.ts`, `state.svelte.ts`, `components/UpdateDialog.svelte`, desktop `update.rs` | Check → available/dismissed → download → install → restart/error. Browser and desktop capabilities differ; installation must use the checked signed update. |
| Activity and undo | `state.svelte.ts`, `components/ActivityPanel.svelte`; entries with undo callbacks | Recorded action → optional undo → outcome. A repeat click must not accidentally execute the same undo concurrently. |
| Interaction surfaces | `components/Select.svelte`, `ActionMenu.svelte`, `CommandPalette.svelte`, `ContextSwitcher.svelte`, `NamespacePicker.svelte`, `ColumnChooser.svelte`, `HelpOverlay.svelte`, `Tour.svelte`, `Sidebar.svelte` | Closed/open, focused/selected, search/typeahead, tour step/dismiss, expanded/collapsed. Keyboard and mouse must trigger the same semantic action and restore focus on dismissal. |
| Ephemeral presentation and persistence | `components/Toasts.svelte`, `TimeChart.svelte`, `Sparkline.svelte`, `LogRate.svelte`, `appearance.ts`, `skin.ts`, `pins.ts`; timers, hover, resize, stored preferences | Show → timeout/dismiss; pointer enter/move/leave; drag → commit; persist → restore/fallback. Dispose timers/listeners and preserve in-memory behavior when storage fails. |

Health classifications in `status.ts`, `health.rs`, `gitops.rs`, `certmanager.rs`,
and `views.rs` describe observed Kubernetes state. Treat them as projections of
external state, rather than introducing local transition logic that could disagree
with the cluster. Pure parsing, formatting, graph building, and diff helpers are
not independent lifecycle owners.

## Concrete follow-up findings

These are source-level findings, not changes shipped by this patch. They deserve
focused regression tests before altering the workflows.

1. `DetailsPanel.load()` has no request generation or target check after its awaits.
   Its timer can overlap a slow request. Capture the target and reject obsolete
   completions, including the subsequent events request.
2. `Creating.start()` awaits schema lookup and then replaces the draft without
   checking whether the user has edited it or requested a different template.
   `CreateView.insert()` also captures the previous text before awaiting. Define
   replacement versus append semantics and protect both with a draft revision.
3. `HelmChangeView.preview()` constructs its review key after awaiting the render.
   Inputs changed during the request can therefore label an older render with
   newer inputs. Capture the complete request/key before awaiting; verify it again
   before execution. Include context identity in the review contract.
4. `Access.#ask()` guards publication by context name, while its `finally` clears
   asking keys unconditionally. An old request can interfere with new work using
   the same keys; A → B → A also defeats name-only freshness checks. Use a monotonic
   generation for reset/refresh and completion bookkeeping.
5. `Session.check()` and `signIn()`/`signOut()` have no shared operation generation.
   Test delayed session responses against login/logout before changing behavior.
6. `AppState.loadK9s()` checks context on successful fetch, but its catch/finally
   can still clear configuration/startup state after a context change. Apply the
   same ownership rule to success, failure, and cleanup.

These are the next highest-value candidates because an explicit transition owner
can improve visible correctness and make both human and agent modifications safer.
They are not a promise that every other machine is free of races.

## Transition contracts for the changes here

| Owner | Event | Result |
| --- | --- | --- |
| HTTP terminal, connecting | write | Buffer, even if the WebSocket is already open. |
| HTTP terminal, connecting | protocol ready | Mark ready, flush input in order, deliver ready. |
| HTTP terminal, connecting/ready | remote exit or disconnect | Mark exited, drop buffered input, deliver one exit. |
| HTTP terminal, any live state | local close | Mark closed, drop input, close transport; late events are ignored. |
| Desktop terminal, awaiting allocation | local close | Close queue immediately; close native session when its ID arrives. |
| Desktop terminal, awaiting allocation | remote exit | Close queue; later allocation cannot start it. |
| Input queue, waiting/ready | close | Irreversibly closed; pending data discarded; already sent input cannot be recalled. |
| MCP approval, waiting | caller canceled | Receiver closes; next pending-list access removes request and answer returns false. |

MCP pruning is lazy: cancellation does not run a background sweeper. A canceled
entry is removed on the next list, answer, enqueue, or timeout cleanup. The UI
already polls pending approvals. The existing two-minute timeout and permission
checks remain in place.

Cancellation here specifically means the Rust confirmation future was dropped.
A client disconnect or MCP cancellation notification does not necessarily drop
that future; this change does not add transport-level cancellation support.

Desktop allocation and protocol readiness are different: the native session ID
allows input writes, even when its channel's ready message is delivered later.
Browser input waits for the ready message. Existing browser resize controls can
still be sent while the server opens the terminal. Local closure is idempotent,
late write errors are suppressed after closure, and native cleanup rejections are
handled because the backend may already have disappeared.

## Verification and maintenance

- `ui/src/lib/api/queue.test.ts`: readiness, coalescing/serialization, close before
  start, close with an in-flight write, write failure recovery and late errors.
- `ui/src/lib/api/terminal.test.ts`: socket-open versus protocol-ready, remote exit,
  disconnect, local close, late events and late input, request ordering, normal
  output/resizing, and a queued open event after closure.
- `ui/src/lib/api/terminal-tauri.test.ts`: late allocation, exit before allocation,
  rejected allocation after closure, normal operation, exit during a write, and
  cleanup failures before/after allocation.
- `crates/kubri-mcp/src/hub.rs`: canceled caller, consumed approval, existing
  open-window requirement, call history, independent list/answer cleanup, and
  preservation of simultaneous live approvals and refusals.

When changing a machine, name its owner, events, terminal states, context/target
identity, retry behavior, and disposal behavior. Test the sequence “start A,
replace/close A, finish A” and both event orders for each race. Preserve backend
authorization even when UI state says an action is available. Avoid automatic
retries of mutations: an uncertain response does not prove the mutation failed.

This audit and unit tests do not substitute for a live-cluster desktop/browser
smoke test. No live cluster mutations were used to validate this change.
