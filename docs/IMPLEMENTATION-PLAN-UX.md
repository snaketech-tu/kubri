# Kubri implementation plan: UX fixes after 1.0

Date: 31 October 2026. Starting point: Kubri 1.0.0 with the auto-updater (PR #66) and the custom
select boxes (PR #67). Status: proposed, nothing implemented yet.

This plan fixes the findings of the UX/UI review of 31 October 2026. That review ran Kubri's web
server against a real k3s cluster with broken demo workloads (CrashLoopBackOff,
ImagePullBackOff, an unschedulable pod, a throttled pod, a HorizontalPodAutoscaler) and went
through every page, light and dark, at 1440 and 820 px. Each finding has an id (**U1** to
**U37**) so pull requests can name what they close.

## Decisions

- **One release per phase**: 1.1 to 1.4. 1.1 holds everything that can make a user act on the
  wrong object; it ships first and alone.
- **No new big features before the cleanup.** Phases 1.1 to 1.3 change existing screens. New
  features (U31 to U37) come last, in 1.4.
- **Language stays English.** The review listed a German interface as missing; the earlier
  decision (plan v2: "English only") stands, so it is out of this plan.
- **k9s keys keep their meaning.** Where an action moves (menu order, footer hints), its key
  does not change.

## Principles

- **One source for actions.** Every action stays in `objectActions` in `ui/src/lib/objects.ts`.
  The action bar, row menu, inspector footer, palette and footer hints all read from it, each
  with its own rule for how many to show (U9).
- **Every fix gets a test.** Logic goes into plain TypeScript modules under `ui/src/lib/` with
  vitest tests; server-side changes get Rust tests in `kubri-core`. Visual fixes are checked with
  screenshots in light and dark at 1440, 1100 and 820 px.
- **Desktop and web stay at parity.** Check the Linux app's engine (WebKitGTK) for every layout
  change; its flexbox differs from Chromium.

## Overview

| Release | Theme | Findings | Pull requests |
| --- | --- | --- | --- |
| 1.1 | Act only on what you chose | U1 to U8 | 4 |
| 1.2 | One place for each thing | U9 to U19 | 5 |
| 1.3 | Say it plainly | U20 to U30 | 4 |
| 1.4 | What was missing | U31 to U37 | 7 |

## 1.1: Act only on what you chose

### PR 1: The action target is always visible and chosen (U1, U2)

- **U1** Today the first row is the cursor as soon as a list opens, and the action bar offers
  "Delete…" for it. After Esc clears the filter, the cursor stays on a row that has scrolled out
  of view, and Ctrl+D opens the delete dialog for it.
  - The cursor is shown only after the user moves it (keys, click, owner jump). Before that the
    action bar shows the list's own actions (Create, Columns, CSV) and object actions are disabled.
  - Whenever the filter, sort or data changes, the cursor row is scrolled into view; if it no
    longer matches the filter, the cursor is cleared instead of kept.
  - The action bar names the target in full ("Selected: shop/checkout-54c9467595-p4f8g", or
    "3 marked") and truncates in the middle, not at the end.
- **U2** Destructive actions (delete, drain, force, uninstall) are never offered for a row that is
  not rendered on screen. `ResourceView.svelte` checks this before opening the dialog.
- Files: `ui/src/lib/components/ResourceView.svelte` (cursor state, lines around `cursorKey`),
  `ActionBar.svelte`, `ui/src/lib/objects.ts`.
- Tests: a list-state module with tests for "filter hides cursor row", "Esc clears filter", "no
  cursor before first move".

### PR 2: Warnings belong to objects that still exist (U3, U4)

- **U3** "Needs attention" on the overview lists BackOff warnings of pods that a rollout already
  replaced, and counts them ("5 need attention" with 2 broken pods).
  - `kubri-core/src/pulse.rs`: drop warnings whose involved object no longer exists, using the
    pod and workload lists Pulses already watches. Keep them for 10 minutes as "resolved" (grey),
    so a quick look still explains what happened.
  - The count in the header matches the cards.
- **U4** "Investigate" on a vanished object shows the raw `pods "…" not found`. Instead the
  inspector says "This pod no longer exists" and, when the owner is known, offers "Show the pods
  of ReplicaSet … / Deployment …".
- Files: `crates/kubri-core/src/pulse.rs`, `ui/src/lib/pulse.ts` (`attentionItems`),
  `ui/src/lib/components/PulsesView.svelte`, `DetailsPanel.svelte`.

### PR 3: Changes that do not stick are flagged (U5, U6, U7)

- **U5** The scale dialog shows the current replicas and, when a HorizontalPodAutoscaler targets
  the workload, says "HPA frontend keeps this between 3 and 6 and will undo the change", with an
  action "Change the HPA limits".
- **U6** The same for objects managed by Flux or Argo CD (the managed note from 0.7 already knows
  this): scale, set image, labels and edit warn that the next sync reverts the change.
- **U7** The delete dialog explains its options in plain words: "Force: skip the grace period;
  the pod may still be running on its node" and "Dependent objects: delete them too
  (recommended) / keep them" instead of "delete in background".
- Files: `ui/src/lib/components/ActionDialog.svelte`, a small `kubri-core` lookup for HPAs that
  target an object (next to `references.rs`), `ui/src/lib/gitops.ts`.

### PR 4: Drilling down never changes the global namespace (U8)

- **U8** Enter on deployment `frontend` opens its pods and sets the namespace picker to `shop`
  without saying so; the rest of the app is then filtered.
  - `app.drill()` in `ui/src/lib/state.svelte.ts` stops passing the object's namespace to
    `open()`; the scope chip ("Pods of Deployment frontend") already narrows the list.
  - Back restores the previous list exactly, including filter, sort and cursor.
- Release 1.1: changelog, version bump.

## 1.2: One place for each thing

### PR 5: Fewer copies of the same actions (U9, U10, U11)

- **U9** The same actions appear four times: action bar, inspector footer, footer key hints (two
  lines at 1440 px, three at 820 px) and row menu.
  - Action bar: the 3 to 4 most used actions of the kind plus "More".
  - Footer hints: at most 5 (`:` command, `/` filter, the view's top two keys, `?` help).
  - Inspector footer: only actions the tabs above do not already offer.
- **U10** "Delete…" leaves the Logs and Shell tabs; it stays in the list and on the inspector's
  overview tab.
- **U11** The row menu (over 30 entries, cut off at the bottom of the window, starting with
  "Copy 'Running'") is reordered by use: View, Connect, Change, Go to, then Copy as a submenu,
  Delete last. It gets a maximum height and scrolls; it opens upwards near the bottom edge.
- Files: `ui/src/lib/objects.ts` (a `priority` per action), `ui/src/lib/menu.ts`,
  `ActionBar.svelte`, `ActionMenu.svelte`, `DetailsPanel.svelte`, `ui/src/routes/+page.svelte`
  (`hints`).

### PR 6: Breadcrumbs that are a path (U12)

- **U12** Breadcrumbs repeat the sidebar group ("Workloads › Pods › Pods of Deployment
  frontend", "Explore › Settings"), and some pages have none.
  - One path everywhere: context › namespace › kind › object › tab, for example
    "prod-eu › shop › Deployments › frontend › Pods". Tool pages: "prod-eu › Helm".
  - Every step is a link and goes back to that place with its filter.
  - Built from the navigation history in `state.svelte.ts`, rendered by `PageHeader.svelte`;
    `navPlace()` in `ui/src/lib/nav.ts` is no longer used for crumbs.

### PR 7: Sidebar grouped by task (U13, U14)

- **U13** Groups: Overview and Search at the top; Workloads; Network (Services, Ingresses,
  NetworkPolicies); Config and storage (ConfigMaps, Secrets, volume claims); Cluster (Nodes,
  Namespaces, Events, CRDs); Tools (collapsible: Topology, What changed, Helm, Access,
  Sanitizer, Right-sizing, Port-forwards, Benchmarks); at the bottom Clusters, Integrations,
  Settings, Help. Collapsed state is kept in the settings file.
- **U14** The k9s command hints (`:fleet`, `:pulses`) show on hover and focus only.
- Files: `ui/src/lib/nav.ts` (`NAV_SECTIONS`), `Sidebar.svelte`.

### PR 8: A clearer header (U15, U16, U17)

- **U15** "CONTEXT SETTINGS" looks like a label. Context settings move into the context menu as
  a gear next to each context; the header keeps only the context name and its colour.
- **U16** "Editable" reads like a state. It becomes a switch labelled "Read-only" with a lock,
  on when changes are blocked, with the reason in its tooltip (guard rail, k9s, context).
- **U17** Two searches (the palette and the Search page): the palette gets a last entry "Search
  all objects for '…' ↵" that opens the Search page with the query; the sidebar keeps Search.
  The palette's placeholder becomes "Search or run…" so it is not cut off.
- Files: `ui/src/routes/+page.svelte`, `ContextSwitcher.svelte`, `CommandPalette.svelte`.

### PR 9: Narrow windows (U18) and keyboard focus (U19)

- **U18** At 820 px the sidebar stays 220 px wide (it collapses only below 720 px), the read-only
  switch and help fall out of the header, tables are cut off. Below 1100 px the sidebar becomes
  an icon rail; below 1000 px the inspector opens as its own page with a back button; header
  items that do not fit go into a "More" menu.
- **U19** After using a header control (namespace picker), `:` and other k9s keys do nothing
  until the user clicks elsewhere. Global keys work whenever focus is not in a text field;
  closing a popover returns focus to the main area.
- Release 1.2.

## 1.3: Say it plainly

### PR 10: Overview and fleet tell the truth at a glance (U20 to U23)

- **U20** Kubelet texts are shortened: "Container checkout keeps restarting (26× in the last
  hour)" instead of "Back-off restarting failed container checkout in pod …_shop(uuid)". The
  whole card is clickable; "Investigate" remains as its visible label.
- **U21** Health bars: healthy part green, failing part red, as segments. A bar 84% full in red
  no longer means "84% healthy".
- **U22** Healthy kinds fold into one line ("7 kinds healthy"); kinds with problems stay on top.
- **U23** Fleet: "unreachable" and "workloads failing" are separate states with separate
  counts; the status dot of the connected cluster is green; "CPU" is labelled "CPU requested"
  on Fleet and "CPU used" on the overview.
- Fix the overlapping "Pods" and "2 failing" labels when the inspector is open (part of U22).
- Files: `ui/src/lib/pulse.ts`, `PulsesView.svelte`, `ui/src/lib/fleet.ts`, `FleetView.svelte`.

### PR 11: Lists show what matters (U24 to U27)

- **U24** CPU and memory against requests and limits get tones: amber above 100% of request,
  red at 90% or more of the limit with a "throttled" tag for CPU. The CPU / limit column is
  shown by default when a pod has limits.
- **U25** Completed pods show "0/1" in grey, not amber; Succeeded is never a warning.
- **U26** The tab counts (All, Needs attention, Running) count the filtered rows.
- **U27** One way to say "nothing": `<none>`, `None` and empty cells become "—"; a headless
  service shows "Headless". "Created At" and other timestamps show as Age with the date in the
  tooltip. The API version next to the title moves into the tooltip of the title.
- Columns that fit: the Namespace column hides when one namespace is chosen, and the default
  pod columns fit 1440 px with the sidebar open.
- Files: `ResourceView.svelte` (`tone`), `ui/src/lib/status.ts`, `ui/src/lib/columns.ts`,
  `ui/src/lib/format.ts`.

### PR 12: Actions that fit the object (U28, U29)

- **U28** Only applicable actions are enabled: Cordon or Uncordon depending on the node, "Show
  pods" disabled with a reason for a service without selector (no red toast), "Install chart"
  explains why it is off (no `helm` found) and how to fix it.
- **U29** Events sort newest first by default, warnings are tinted, the message wraps.
- Files: `ui/src/lib/objects.ts` (an `available(row)` per action), `HelmView.svelte`,
  `ui/src/lib/columns.ts`.

### PR 13: Inspector, logs, create and the tool pages (U30)

- Inspector: the problem card stays first; "What happened" starts with warnings; "Phase
  Running" is hidden while a container is crash-looping; the deployment inspector lists
  containers and images; the footer no longer covers content.
- Logs: one toolbar row. Follow is a toggle, search with its "only matches" switch next to it,
  timestamps, wrap, fold and table move into a "View" menu. "Only matches" is off until there is
  a search.
- Create: choosing a kind adds its template at once; the namespace is a picker; a context other
  than the current one is marked in amber.
- Sanitizer sorts by errors, then warnings, then score. Topology shows objects with issues first.
- Empty states get an action: Port-forwards "Start a port-forward…", Integrations a "How to
  install" link per tool.
- Settings gets a section list on the left; "Reload" becomes "Reload k9s files".
- Help: `?` opens the palette in key mode; typing filters keys and commands.
- Release 1.3.

## 1.4: What was missing

Each of these is its own PR and can ship in any order.

- **U31 Pins and recents.** Pin kinds, namespaces and objects; a "Recent" group in the sidebar
  and the palette. Kept per context in the settings file.
- **U32 Actions in the palette.** Every action from `objectActions` for the current row,
  plus app commands (dark mode, read-only, zoom), searchable by name.
- **U33 Session activity with undo.** A panel listing every change made in this session (what,
  where, when, result). Scale, set image, labels and restart offer Undo where Kubernetes allows
  it. Toasts link to the entry.
- **U34 Related objects in the inspector.** Deployment → its pods; Service → endpoints and
  pods; Pod → its services, ConfigMaps, Secrets and volume claims, from `references.rs`.
- **U35 Rollout history and rollback** for deployments, statefulsets and daemonsets: revisions
  with an image diff, "Roll back to revision 3" through a dry-run diff.
- **U36 Trends in lists.** A small CPU and memory sparkline per row from the samples Pulses
  already keeps.
- **U37 First run.** A short tour (palette, `:`, right click, read-only) on first start, and a
  clear page with next steps when no kubeconfig is found. Can be reopened from Help.
- Release 1.4.

## Verification for every release

- Re-run the review scenario: k3s with the demo workloads (CrashLoopBackOff, ImagePullBackOff,
  Pending, throttled pod, HPA, CronJob) and check each closed U-number by hand.
- Screenshots in light and dark at 1440, 1100 and 820 px of Overview, Pods with inspector, Logs
  and the changed dialogs, plus a pass in WebKitGTK.
- `pnpm check`, `pnpm test`, `cargo test`, clippy with the CI toolchain.
