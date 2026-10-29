# Kubri redesign implementation plan

Date: 29 October 2026. Status: changesets 1–8 implemented in the application.

**Brand decision: 02 / Link.** The angular K mark and electric-blue identity are selected by the project owner. Span and Portal remain archived alternatives. This decision supersedes the original review's recommendation of Span.

[Selected mark](logos/link.svg) · [Updated prototype](index.html) · [Review and source evidence](REVIEW.md)

## Outcome

Ship a consistent Kubernetes workspace in the existing Svelte application: clear cluster context, prioritized operational problems, a continuous pod → events → logs workflow, and reliable editing and destructive actions. Preserve the existing functionality, k9s shortcuts, aliases, plugins, skins, and desktop/web parity.

The prototype supplies layout and interaction direction. Implement reusable Svelte components against the real state and backend contracts; do not copy its synthetic data or demo-only behavior into the product. Keep the current English product language and generic resource/CRD support.

## 1. Brand and design foundation

**Decision made; production assets and application styling still to implement.**

- Use Link's geometric K as the mark and `kubri` as the lowercase wordmark; retain the product name `Kubri` and existing application identifier.
- Primary brand color: **#4B64F4**. Light supporting blue: **#9DAAFF**. Use **#354AC4** for smaller accent text on light backgrounds and **#A6B3FF** as the dark-theme interaction accent.
- Use cool neutral surfaces: light background **#F6F7FB**, dark background **#101522**. Reserve green, amber and red for operational status. A selected blue row must not imply a healthy resource.
- Finalize one vector master, wordmark lockups, single-color/reverse marks and app-icon compositions. The current SVG is a geometric concept; use the selected Link direction from the exploration board as the reference for final optical refinement, rather than treating the PNG as a production master.
- Check silhouettes and stroke/space balance at 16, 24, 32, 128 and 256 px. Export the required PNG, ICO and ICNS assets. Resolve wordmark lettering and font licensing before packaging.
- Update `ui/src/lib/assets/logo.svg`, `ui/src-tauri/icons/*`, the browser favicon, `docs/assets/kubri-logo.svg`, `docs/assets/kubri-logo-dark.svg`, and README/release imagery together.

**Acceptance:** the same Link identity appears in the application header, favicon, installers, taskbar/dock and documentation. Light, dark and monochrome versions remain legible at small sizes. Brand colors are independent of success/warning/error tokens. Existing k9s skins continue to override application colors without breaking focus visibility or status meaning.

## 2. Protect work and clarify actions — first application changes

Addresses the two P1 findings and the destructive-dialog finding.

**YAML edits**

- Expose dirty/saving state from `YamlEditor.svelte`; coordinate leaving an edit through one guard in `DetailsPanel.svelte` and `state.svelte.ts`.
- Route close, Cancel, Escape, object selection, tab changes, history navigation, namespace/context changes and sign-out through the same decision: keep editing, discard, or save and continue.
- A failed save keeps the draft and current context. Preserve resource identity and version during saves. Handle native-window/browser closing with their supported unsaved-work protections; do not promise an asynchronous save after the process has already closed.

**Deletion**

- Restrict “Delete finished pods” to successfully completed pods. Offer failed/stuck cleanup separately with an explicit, inspectable selection; `Pending` and crash-looping pods must never be silently included as completed.
- Share the same selection logic between toolbar, context menu, `z` shortcut and bulk actions.
- Show context, namespace, count, names and statuses before confirming. Start keyboard focus on Cancel. Use additional confirmation for production or bulk operations, not for every routine action.
- Bind confirmation to the reviewed context and target snapshot. Reject/invalidate it if the context changes before execution. Keep existing server/core guards authoritative.

**Touchpoints:** `YamlEditor.svelte`, `DetailsPanel.svelte`, `ResourceView.svelte`, `ActionDialog.svelte`, `objects.ts`, `actions.ts`, `state.svelte.ts`; transport/core changes only where the context-binding guarantee needs them.

**Acceptance:** edit YAML and try every exit route; the draft survives cancellation and failed saving. A completed/crashed/pending fixture produces exactly the advertised cleanup selection. Enter on opening a destructive dialog does not submit it. A context change cannot redirect a previously reviewed action.

## 3. Make data and connection states truthful

Addresses ambiguous empty/error states, disappearing metrics and request/limit clarity.

- Define explicit list states: initial loading, live, empty, filtered-empty, stale/reconnecting, denied and failed. Retain last successful rows when appropriate; never present a failed initial request as “no resources”.
- Track the last successful update separately from transport connectivity. A quiet watch is not evidence of disconnection. Clear stale data across context changes so one cluster's rows cannot appear under another cluster's name.
- Give retry a clear lifecycle: one subscription, cancellation of the old watch, no duplicate updates. If needed, extend the shared event/error contract across Rust core, HTTP/WebSocket and Tauri adapters together.
- Keep metrics columns stable. Distinguish unsupported metrics, permissions, temporary failure and a genuinely measured zero. Preserve last successful values with explicit age/stale labeling.
- Explain CPU/memory usage relative to requests, limits and node capacity. Missing requests or limits produce an explicit unavailable ratio, never division by zero. Above-request utilization alone is not a hard-limit breach.

**Touchpoints:** `watch.svelte.ts`, `ResourceView.svelte`, `api/types.ts`, `api/http.ts`, `api/tauri.ts`, `crates/kubri-core/src/table.rs`, relevant server event serialization, `format.ts`, `status.ts`.

**Acceptance:** deterministic fixtures cover first-load denial, reconnect with existing rows, successful empty data, filter misses, absent metrics-server, partial metrics, recovery and context changes. Both transports expose the same user-facing meaning. Columns do not jump when a metric request fails.

## 4. Build the application shell and shared components

Depends on the selected branding; consume the state contracts from step 3.

- Introduce semantic light/dark tokens in `app.css`: surfaces, text, borders, accent, selected state, focus, status and chart series. Build only shared primitives the real screens need: buttons, status indicators, filters, empty/error states, detail tabs and dialogs.
- Reorganize the sidebar into Overview, Workloads, Cluster and Explore. Keep all resources and CRDs discoverable; retain `:pulses`, `:xray`, aliases and custom commands even when navigation labels change.
- Keep context, namespace and effective read-only mode visible. Allow explicit environment labels per context; do not infer production protection solely from names.
- A voluntary read-only preference can make the app more restrictive. It must not turn off server/k9s-enforced read-only behavior. Establish that precedence before exposing a mode toggle.
- Add an adaptive master/detail layout with a bounded/resizable inspector on larger windows and a dedicated detail view on narrower windows. Preserve list selection, filters, sort and scroll when returning.
- Use semantic controls, visible focus and context-appropriate shortcuts. Respect reduced motion and system text scaling.

**Touchpoints:** `routes/+page.svelte`, `Sidebar.svelte`, `ContextSwitcher.svelte`, `NamespacePicker.svelte`, `CommandPalette.svelte`, `ActionBar.svelte`, `skin.ts`, `state.svelte.ts` and shared styles.

**Acceptance:** validate 1440×900, 1280×800 and the existing Tauri minimum of **900×560**. Validate browser widths 768, 390 and 320 px as well. No page-wide horizontal overflow; tables/logs can scroll within their own regions. Check light, dark, a custom k9s skin and 200% zoom. All existing navigation destinations remain reachable.

## 5. Implement the primary investigation workflow

Depends on steps 2–4. Deliver as two reviewable changes.

**Overview and pod inspector**

- Rework `PulsesView.svelte` to surface actionable signals before the complete inventory. Derive signals from available Kubernetes state, events and metrics. Link each signal to the exact object/context; handle objects that disappear during investigation.
- Avoid counting a Deployment, ReplicaSet and Pod as three independent incidents when they describe the same failure; group related signals when ownership evidence is available.
- Keep cluster-wide and namespace-scoped values distinguishable. Retain session-only history unless a real historical store is introduced in a separate project.
- Add visible status filtering and readable metric headings to `ResourceView.svelte` while keeping advanced k9s filters. Display status with text/icon as well as color.
- Reorganize `DetailsPanel.svelte` / `Describe.svelte`: status summary, key facts, containers, events and YAML. Show previous-container logs prominently for applicable failure states. Support workloads, CRDs and multiple containers without hardcoding the sample email pod.

**Logs**

- Keep context, namespace, object and container visible. Clearly distinguish previous terminated logs, current live streams, paused autoscroll and an ended/disconnected stream.
- Preserve existing merged sources, time ranges, timestamps, wrap, filtering, marks, copy/export and shortcuts. Do not silently change which container instance a view represents.
- Keep the prototype's “First clue” text out of the shipping product unless backed by an explicitly implemented deterministic diagnostic. Treat structured log-level filtering as optional follow-up; unstructured logs must remain fully visible.

**Acceptance:** a user can travel from a real warning to the correct pod, event and previous log instance without reselecting context. Test multi-container pods, controller drill-down, missing objects and unstructured log output. Returning to the resource list restores its state. No synthetic content appears in production views.

## 6. Connections and remaining feature surfaces

- Improve empty startup, missing/invalid kubeconfig, expired credentials, unavailable cluster and permission-denied experiences. Provide actionable retry/context-selection paths.
- Adapt onboarding to the runtime: desktop can use local kubeconfig; the web app uses the server's connection/authentication model. Do not introduce a browser kubeconfig upload as an assumed equivalent.
- Carry the same patterns into XRay/Topology, Helm, RBAC, Sanitizer, port-forwards, benchmarks, settings, help and sign-in. Preserve running terminals, forwards and benchmarks according to their current lifecycle rules.
- Update accessibility semantics: sorting buttons with `aria-sort`, labeled selection controls, intentional table keyboard navigation, dialog focus containment/return and announcements for meaningful state changes.

**Acceptance:** a feature inventory confirms no existing capability or keyboard entry point was dropped. Runtime-specific onboarding is accurate. Established sessions and background operations are not accidentally terminated by presentational navigation changes.

## Delivery sequence and dependencies

Suggested changesets; these are planned work items, not created branches or PRs.

| Order | Changeset | Depends on | Completion evidence |
| --- | --- | --- | --- |
| 1 | Safe cleanup selection and context-bound destructive dialogs | Existing APIs | Target-selection, focus and context-change behavior checks |
| 2 | Central YAML draft/exit protection | Existing editor/state | All exit routes plus failed-save coverage |
| 3 | List/metrics state contracts and recovery | Existing transports | Matching HTTP and Tauri behavior; error/empty fixtures |
| 4 | Final Link assets and semantic design tokens | Brand decision, already made | Icon exports, light/dark/skin checks |
| 5 | Shell, navigation and adaptive inspector container | 2–4 | Layout, keyboard and state-preservation checks |
| 6 | Overview, resource list and pod inspector | 1–5 | Complete warning → object → events flow |
| 7 | Logs and runtime-aware connection experience | 3, 5, 6 | Previous/current/multi-container logs; onboarding states |
| 8 | Remaining views, accessibility, documentation and release QA | 1–7 | Feature-parity checklist and release validation |

Changesets 1–3 can be shipped before the visual redesign. Each changeset should leave both desktop and browser usable. Scope structured log parsing or backend diagnostic enrichment separately if it exceeds the existing data contracts; neither blocks the core redesign.

## Verification and release gate

- Add focused behavior tests for selection, dirty edits, navigation guards, connection state and context-bound actions. Use browser interaction tests where unit tests cannot verify focus or interaction behavior.
- For UI changes, run the existing `pnpm check`, `pnpm test` and `pnpm build` in `ui/`. For Rust changes, also run `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test`.
- Run real application checks against a disposable local Kubernetes cluster: healthy workload, CrashLoopBackOff, scheduling failure, denied reads, unavailable metrics and interrupted connection. Verify both web and Tauri. Destructive checks use only disposable fixture resources.
- Inspect representative screenshots at the agreed dimensions, light/dark and custom skin. Validate tab order, focus return, screen-reader labels and color contrast; automated overflow checks alone are insufficient.
- Compare the feature inventory and common k9s workflows before/after. Measure task steps/time during a small usability pass rather than claiming unmeasured percentage improvements.
- Refresh documentation and screenshots after behavior stabilizes. Package/install the desktop targets and verify the web build and new Link icons. Keep each changeset reversible; avoid a single migration spanning the entire application.

## Status at handoff

- [x] Review and interactive concept exist.
- [x] Owner selected **02 / Link**; previous Span recommendation superseded.
- [x] Prototype default mark, blue palette and favicon aligned with Link.
- [x] Implementation order, source touchpoints and acceptance criteria documented.
- [x] Link mark integrated as app logo, favicon, desktop icons (`scripts/export-brand.cjs`) and documentation logos. Wordmark lettering and font licensing still need a final review before packaging.
- [x] Changesets 1–3: completed-only pod cleanup, context-bound destructive dialogs (core rejects a target from another context), central YAML draft guard for every exit route, truthful list/metrics states with retry that keeps rows.
- [x] Changesets 4–7: semantic tokens with light/dark toggle, sidebar with Workloads/Cluster/Explore and icons, labeled context/namespace bar with environment label and voluntary read-only, status footer, overview with KPI cards and grouped attention list, pod list with status tabs and icons, inspector with problem callout, events timeline and Events tab, logs with instance selection and stream state, runtime-aware connect screen.
- [x] Browser checks: `node scripts/check-ui.cjs <playwright> [browser] [base-url]` against a built UI (`vite preview`) with fixtures, including 1440/1280/900×560/768/390/320 widths.
- [x] Changeset 8: shared `PageHeader` and `SearchField` components and `.btn`/`.segmented`/`.notice` styles carry the breadcrumb, title and card layout into Topology (XRay), Helm, Access (RBAC), Sanitizer, port-forwards, benchmarks and k9s config. Accessibility: the help overlay and command palette are modal, keep and return the focus, and the palette is a combobox with a listbox of options; empty states announce themselves. README screenshots refreshed from a disposable k3s cluster; Helm release/history/values, port-forward and benchmark flows checked there in light and dark at 1440, 900×560 and 390 px. Desktop builds come from CI on every platform.

No delivery dates or effort estimates are committed here. Establish them from the first changesets and the depth of any required transport changes.
