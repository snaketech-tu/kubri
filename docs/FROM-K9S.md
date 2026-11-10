# Coming from k9s

Kubri does what k9s does, with the same keys, and reads your k9s configuration from the same files. If k9s is in your fingers, you can use Kubri right away. This page covers the first few minutes: what carries over, what works a little differently, and what there is besides.

## Start

1. [Install Kubri](../README.md#install). The desktop app needs nothing else.
2. Start it. Kubri reads the same kubeconfig as kubectl and k9s (`KUBECONFIG`, or `~/.kube/config`) and starts with your `current-context`.
3. It also reads your k9s aliases, hotkeys, plugins, skins and per-context settings, from the same folders k9s uses. `:config` shows what it found and what it could not read. Kubri never writes to your k9s files.

Kubri opens on the **Overview** of the cluster, where k9s opens on pods. <kbd>:</kbd> `po` <kbd>Enter</kbd> gets you to the pods, as in k9s.

## The keys you know

These work as in k9s:

| Keys | What they do |
| --- | --- |
| <kbd>:</kbd> | Go to a kind or run a command: `:po`, `:deploy kube-system`, `:po shop app=web`, `:ctx prod`, `:ns default`, `:pods @staging` for another context, and your own aliases |
| <kbd>/</kbd> | Filter: a regular expression, `!` for the rows that do not match, `-f` fuzzy, `-l app=web` labels |
| <kbd>Enter</kbd>, <kbd>Esc</kbd> | Drill down, go back |
| <kbd>d</kbd> <kbd>y</kbd> <kbd>e</kbd> | Describe, YAML, edit |
| <kbd>l</kbd> <kbd>p</kbd> <kbd>s</kbd> <kbd>a</kbd> | Logs, previous logs, shell, attach |
| <kbd>Ctrl</kbd>+<kbd>D</kbd> | Delete, after asking |
| <kbd>Shift</kbd>+<kbd>F</kbd> | Port-forward |
| <kbd>Space</kbd>, <kbd>Ctrl</kbd>+<kbd>Space</kbd> | Mark rows, mark a range |
| <kbd>Shift</kbd>+<kbd>N</kbd>, <kbd>A</kbd>, <kbd>S</kbd>, … | Sort by name, age, status and the other columns |
| <kbd>Ctrl</kbd>+<kbd>Z</kbd> | Only the faulty rows |
| <kbd>?</kbd> | Every key of the view you are in |

The views keep their k9s commands, though some have a name of their own in Kubri: `:pulses` opens the **Overview**, `:xray` the **Topology**, `:popeye` the **Checks** and `:rbac` the **Access** page. [What Kubri covers](K9S.md#what-kubri-covers) lists every key and command.

## What works a little differently

- <kbd>Ctrl</kbd>+<kbd>K</kbd> opens the command palette, like <kbd>:</kbd>. It does not delete without asking.
- There is nothing to quit: close the window. `:q` opens ResourceQuotas.
- Logs and shells open in the inspector beside the list, so the list stays in view. **Move to the workspace** keeps them running below while you browse elsewhere ([the workspace](DEBUGGING.md#the-workspace)).
- The keys stay with the list while the inspector is open, so <kbd>↑</kbd> <kbd>↓</kbd> show the next object. <kbd>Tab</kbd> goes into the inspector: <kbd>←</kbd> <kbd>→</kbd> change its tab, <kbd>↑</kbd> <kbd>↓</kbd> and the page keys scroll, <kbd>Tab</kbd> walks its links and buttons, and <kbd>Esc</kbd> goes back to the list.
- In a shell, <kbd>Esc</kbd> goes to the program in it. Press <kbd>Esc</kbd> twice quickly to give the keys back to Kubri.
- Changes such as edits, scaling and Helm upgrades show a server-side dry run with the diff first, and each shows the `kubectl` command that does the same.
- Everything also works with the mouse: a click selects a row, a double click opens it, a right click shows everything there is to do with it, and the key hints at the bottom can be clicked.

## What there is besides

- An **inspector** beside the list with the status, the likely problem and a fix to apply, events, logs and related objects ([Logs and debugging](DEBUGGING.md)).
- **What changed**: rollouts, scaling and config changes as a timeline with diffs ([What changed](CHANGES.md)).
- **Create** from templates, forms, files or the clipboard, and Helm installs and upgrades with a diff ([Creating and changing](CREATE.md)).
- **Clusters**: all your contexts on one page, search across them, and contexts that stay connected so switching back is quick ([Several clusters](CLUSTERS.md)).
- Prometheus, Grafana, Loki, Flux, Argo CD and cert-manager, found without setup ([Integrations](INTEGRATIONS.md)).
- A name, color and production marker per context, and read-only or locked contexts ([Settings and guard rails](SETTINGS.md)).
- Snapshots to look at a cluster offline, and `kubri lint` for CI ([Snapshots](SNAPSHOTS.md), [Command line](CLI.md)).
- The same app in the browser, also for a whole team with each person's own permissions ([In the browser](WEB.md)).

## Your configuration

[Using your k9s configuration](K9S.md) says where Kubri looks, how it reads aliases, hotkeys, plugins, skins and jumps, and [what is not supported](K9S.md#not-supported).

Something you use in k9s missing or working differently? **Report a problem** at the bottom of the help (<kbd>?</kbd>), or `:feedback`, opens an issue with your version filled in.
