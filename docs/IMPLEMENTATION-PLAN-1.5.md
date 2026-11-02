# Kubri implementation plan: 1.5

Date: 2 November 2026. Starting point: Kubri 1.4.2 (main at `6d9b158`). Status: proposed,
nothing implemented yet.

1.5 adds two things:

- **Fast context switching** (**F1** to **F4**): going back to a context you used a moment ago
  is instant, and you land where you left it.
- **A local terminal** (**T1** to **T5**): a shell on your own computer in the workspace, with
  `kubectl` and `helm` already pointing at the context and namespace Kubri shows.

## Decisions

- **Still one connected context.** Kubri keeps a few recent contexts warm in the background, but
  lists, actions and the guard rails stay with the one in the top bar. Merged lists across
  clusters stay out of scope, as decided in plan v2.
- **The local terminal is a desktop feature.** In the web app it would be a shell on the server,
  for everyone who can sign in. It stays off there unless the operator turns it on with
  `KUBRI_LOCAL_TERMINAL=true`, and `deploy/kubernetes.yaml` never does.
- **No bundled `kubectl`.** The terminal uses the `kubectl` and `helm` on the user's `PATH`,
  found with the login shell's environment that the desktop app already loads
  (`ui/src-tauri/src/shell_env.rs`). Kubri says when one is missing or too far from the
  cluster's version, with a link to install it. This keeps the app small.
- **No credentials written to disk.** The terminal's `KUBECONFIG` puts a small overlay file in
  front of the user's own files. The overlay holds only `current-context` and the context entry
  with the namespace; the user and cluster entries still come from the user's files.

## Principles

- **Reuse what is there.** Sessions and discovery in `kubri-core` (`Session`, `connect`,
  `read_session`), the PTY code of k9s plugins (`plugin.rs`, `portable-pty`), the `KUBECONFIG`
  value built for helm and plugins (`kubeconfig.rs`), the workspace (`ui/src/lib/dock.svelte.ts`)
  and the xterm.js terminal (`ui/src/lib/terminal.ts`).
- **Guard rails cover the terminal too, as far as they can.** Kubri cannot check commands typed
  into a shell, so it says so, and read-only and locked contexts do not get one (T3).
- **Every change gets a test.** Rust tests in `kubri-core` and `kubri-server`, vitest tests for
  UI logic, and a check against the k3s test cluster with two contexts.
- **Desktop and web stay at parity** for context switching. Check the Linux app's engine
  (WebKitGTK) for layout changes.

## Overview

| Id | Item | Where | Pull request |
| --- | --- | --- | --- |
| F1 | Pool of warm sessions | core | 1 |
| F2 | Check a warm session on switch, refresh discovery in the background | core | 1 |
| F3 | Remember the view per context | UI | 2 |
| F4 | Keep workspace tabs of other contexts, show rows at once | UI | 2 |
| T1 | Local shell in a PTY with an overlay `KUBECONFIG` | core, desktop | 3 |
| T2 | Terminal tab in the workspace, `:term`, <kbd>Ctrl</kbd>+<kbd>`</kbd> | UI | 4 |
| T3 | Guard rails for the terminal | core, UI | 3, 4 |
| T4 | `kubectl` and `helm` found, version skew shown | core, UI | 3, 4 |
| T5 | Terminal settings: shell, start folder, font size | core, UI | 4 |
| — | Docs and release 1.5.0 | docs | 5 |

PRs 1 and 3 do not depend on each other; 2 builds on 1 and 4 on 3. Release PR 5 merges last.

## Fast context switching

Today `switch_context` builds a new client, asks for the server version and runs discovery
every time, and the UI throws away its view: it opens Pods in the context's default namespace,
clears the back and forward history and closes the workspace tabs of the last context. Switching
between two clusters during an incident costs a few seconds and the place you were at, each time.

### F1. A pool of warm sessions

- Replace `others` (connections to other contexts for reading, kept 5 minutes) with one pool of
  sessions by context name, with the time each was last used.
- On a switch, the session being left goes into the pool instead of being dropped. Its client,
  discovery, OpenAPI schemas, access cache and found integrations stay with it.
- The pool keeps the **3** most recently used contexts (setting **Keep recent contexts
  connected**, 0 to 5; 0 turns it off) and drops a session unused for **30 minutes**.
- Reloading the kubeconfig, adding or removing a context, and changing a context's settings
  empty the pool for that context, as `others.clear()` does now.
- `read_session` (compare, fleet, search) takes from the same pool, so a context you just
  compared is also quick to switch to.
- Nothing runs for a pooled session: no watches, no recorder. It holds a client and cached
  data only.

### F2. Checking a warm session

- `switch_context` with a pooled session asks the API server for its version with a short
  timeout (2 seconds). When that works, the switch is done; when it fails with an auth error or
  times out, Kubri connects afresh as today, so expired tokens behave as before.
- Discovery runs again in the background. When the served resources changed (a CRD was
  installed meanwhile), the core sends a `resources-changed` event and the UI reloads the
  sidebar.
- The "What changed" recorder and the favorite port-forwards restart on every switch, as today.
- Tests: a pooled session is reused (no second discovery call against a fake API server), a
  failed version check falls back to a new connection, the pool keeps at most N and drops old
  ones, a kubeconfig reload empties it.

### F3. Remember the view per context

- On leaving a context, the UI keeps its page, selected kind, namespace, list filter, scope,
  selected row and back and forward history, by context name, for the session of the app (not
  saved to disk).
- On coming back, it restores them. When the kind no longer exists or the namespace is gone,
  it falls back as today (Pods, the context's namespace).
- The context switcher marks warm contexts and lists the previous context first, so
  <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>C</kbd> and <kbd>Enter</kbd> go back to it; `:ctx -` does
  the same, like `cd -`.
- Tests: vitest for the saved and restored view, including the fallbacks.

### F4. Workspace tabs and rows on return

- Workspace tabs of other contexts are no longer closed. They keep running, and each tab shows
  its context's name and color, so a shell in production is never mistaken for one in staging.
  A tab of another context that is not visible keeps streaming. Removing a context, or a
  kubeconfig reload that drops it, closes its tabs.
- When a list of a warm context starts, the UI shows the rows it had when you left, dimmed with
  "Updating…", until the watch's first list arrives. Actions stay disabled until then, so
  nothing acts on a stale row.
- Tests: vitest for tab keeping and the stale-rows state; a k3s check that switches between two
  contexts and back with a log tab open.

## Local terminal

### T1. A shell on your computer

- New `kubri-core` module `local_terminal.rs`, built on the PTY code of `plugin.rs`. It returns
  the same `Terminal` (input, output, resize) as container shells, so the desktop app and the
  server carry it with the code they have.
- The shell: the setting **Shell** when set; otherwise `$SHELL` as a login shell on macOS and
  Linux, and PowerShell 7 (`pwsh`) or Windows PowerShell on Windows. `wsl.exe` works as a
  setting.
- The environment: the login shell's environment the desktop app loaded at startup, plus
  - `KUBECONFIG` = the overlay file, then the value already built for helm and plugins (the
    user's files, then the contexts added in Kubri);
  - `KUBRI_CONTEXT` and `KUBRI_NAMESPACE`, for prompts;
  - `TERM=xterm-256color`.
- The overlay file sits in Kubri's config folder (mode `0600`), named per tab and removed when
  the tab closes or Kubri starts. It contains `current-context` and a copy of the context entry
  with the namespace Kubri shows (none for all namespaces), nothing else. kubectl and helm merge
  `KUBECONFIG` with the first file winning, so the user and cluster entries still come from the
  user's files.
- A terminal keeps the context and namespace it started with. Switching the context in Kubri
  does not change an open shell; a new terminal follows the new context.
- Tests: the overlay file's content, and in the k3s test `kubectl config current-context` and
  `kubectl get pods` inside the terminal answer for the chosen context and namespace, while the
  user's kubeconfig points somewhere else.

### T2. The terminal tab

- The workspace gets a third kind of tab, **Terminal**, beside logs and shells, titled
  `Terminal · context/namespace`.
- Open it with `:term` (also `:terminal`), the command palette, a terminal button in the header,
  or <kbd>Ctrl</kbd>+<kbd>`</kbd>. A second press focuses the open terminal of this context
  instead of opening another; <kbd>Shift</kbd> opens a new one.
- The tab uses the existing xterm.js setup: copy, paste, search, links, resize.
- The web app shows the command only when the server allows it (see Decisions).

### T3. Guard rails

- **Read-only and locked contexts** get no local terminal. The command explains why, and
  **Unlock** opens one for the unlock's 15 minutes. The shell is not closed when the unlock
  ends, but its tab says the context is locked again.
- **Production contexts** open the terminal with the production banner on the tab and a first
  line in the shell: "Commands here run against <context> without Kubri's checks."
- The `--read-only` flag and `KUBRI_READ_ONLY` turn the local terminal off entirely.
- AI agents over MCP cannot open a local terminal.

### T4. kubectl and helm

- When a terminal opens, the core looks up `kubectl` and `helm` on the terminal's `PATH` and
  runs `kubectl version --client`.
- The tab's header shows the versions. When `kubectl` is missing, or its minor version is more
  than one away from the server's (outside the supported skew), it shows a short note with a
  link to the install page. The shell opens either way.

### T5. Settings

Under **Settings > Terminal**: shell (path and arguments), start folder (home by default), font
size, and **Keep recent contexts connected** from F1 under **Settings > Contexts**. Saved in
`settings.json` like the other settings; documented in `docs/SETTINGS.md`.

## Docs and release

- New `docs/TERMINAL.md`: what the terminal sets up, the overlay file, the guard rails, the web
  setting, Windows and WSL.
- `docs/CLUSTERS.md`: warm contexts and the previous-context key.
- README: one line under "Act" or "Run it anywhere" for each.
- Release 1.5.0 bumps the three version lines only (`Cargo.toml`, the `kubri-*` entries in
  `Cargo.lock`, `ui/package.json`).

## Checks before release

- k3s with two contexts (two namespaces' service accounts, or k3s plus a second cluster):
  switch back and forth with a log tab and a terminal open; switch after deleting the second
  cluster's token, which must fall back to a fresh connection and show the error.
- Windows (on the user's PC): the terminal opens PowerShell, `kubectl` answers for the chosen
  context, and WSL works when set.
- macOS app started from Finder: the terminal finds Homebrew's `kubectl`.
