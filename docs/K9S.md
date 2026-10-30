# Using your k9s configuration

Kubri reads the aliases, hotkeys, plugins, skins, jumps and settings you set up for k9s, from the same files, so they work in Kubri without changes. `:config` in the command palette shows what was found, which files could not be read and why, and lets you pick a skin.

## Where Kubri looks

The same places k9s uses:

| | Linux | macOS | Windows |
| --- | --- | --- | --- |
| Config | `~/.config/k9s` | `~/Library/Application Support/k9s` | `%LOCALAPPDATA%\k9s` |
| Data | `~/.local/share/k9s` | `~/Library/Application Support/k9s` | `%LOCALAPPDATA%\k9s` |

`XDG_CONFIG_HOME` and `XDG_DATA_HOME` move them, and `K9S_CONFIG_DIR` puts everything in one directory, as in k9s.

| File | What it holds |
| --- | --- |
| `aliases.yaml`, `hotkeys.yaml`, `plugins.yaml` | In the config or data directory. |
| `plugins/*.yaml` | More plugins, one or more per file. |
| `skins/<name>.yaml` | Skins. |
| `jumps.yaml` | Where <kbd>Enter</kbd> leads from a resource; see [Jumps](#jumps). |
| `config.yaml` | Settings; see [Settings](#settings). |
| `clusters/<cluster>/<context>/` | In the data directory: aliases, hotkeys, plugins and `config.yaml` (with `k9s.skin`) for one context, which win over the global ones. |

Kubri reads the files again when you switch contexts or press Reload on the `:config` page.

## Settings

Kubri takes these settings from `k9s:` in `config.yaml`:

| Setting | What it does in Kubri |
| --- | --- |
| `readOnly` | Nothing that changes the cluster is offered or allowed: no edits, deletes, scaling, restarts, shells, dangerous plugins and so on. A context's `config.yaml` can turn it on or off for that context. Starting Kubri with `KUBRI_READ_ONLY=true` makes it read-only whatever the files say. |
| `defaultView` | The view Kubri opens when it starts, such as `deploy` or `pods kube-system`. |
| `ui.skin` | The skin; a context's `config.yaml` uses `k9s.skin`. |
| `thresholds.cpu`, `thresholds.memory` | `warn` and `critical` percentages that color the %CPU and %Mem columns. |
| `logger.tail`, `logger.sinceSeconds` | Where the log view starts: the last lines, or the last seconds when `sinceSeconds` is above 0. |
| `logger.textWrap`, `logger.showTime`, `logger.disableAutoscroll` | Wrapping, timestamps and following new lines in the log view. |
| `portForwardAddress` | The address port-forwards listen on; `K9S_DEFAULT_PF_ADDRESS` wins over it. |

## Jumps

```yaml
jumps:
  example.com/v1/patches:
    targetGVR: batch/v1/jobs
    labelSelector: "patch={{.metadata.name}}"
    targetNamespace: ""        # the object's namespace; `all` or a name also work
```

<kbd>Enter</kbd> on an object lists what belongs to it: the pods of a Deployment, StatefulSet, DaemonSet, ReplicaSet, Job or Service, the Jobs of a CronJob, the pods on a node, the pods in a namespace and the objects of a CRD. `jumps.yaml` adds your own, or replaces these, with a label and/or field selector. Selectors and the namespace may use fields of the object like `{{.metadata.labels.app}}` and `{{index .metadata.labels "app.kubernetes.io/name"}}`. <kbd>Esc</kbd> goes back.

## Annotations

Like k9s, Kubri reads two annotations on pods and pod templates:

- `k9scli.io/port-forwards: "8080, 9090:http, nginx::8443:443"` picks the port the port-forward dialog suggests: a container port by number or name, a local port in front of it, and a container with `::`.
- `k9scli.io/auto-port-forwards` in the same form starts those port-forwards right away on <kbd>Shift</kbd>+<kbd>F</kbd>, without the dialog.

## Aliases

```yaml
aliases:
  pp: v1/pods
  crb: rbac.authorization.k8s.io/v1/clusterrolebindings
  fred: pod kube-system app=dns
```

`:pp` then opens pods, and `:fred` the pods in `kube-system` with the label `app=dns`. An alias can stand for any command, including another alias, `xray deploy` or `ctx prod`. Words after an alias are passed on, so `:pp shop` opens pods in `shop`.

Every resource command takes the same extras, aliased or not: a namespace, labels as `key=value` (or `-l key=value`) and `@context` to switch contexts first, like `:deploy shop app=web @prod`.

## Hotkeys

```yaml
hotKeys:
  shift-0:
    shortCut: Shift-0
    description: Viewing pods
    command: pods
```

A hotkey runs its command as if typed after `:`. The command may use the plugin variables below, such as `$NAMESPACE`.

## Plugins

```yaml
plugins:
  stern:
    shortCut: Ctrl-L
    description: Logs <Stern>
    scopes: [pods]
    command: stern
    args: [--tail, 50, $FILTER, -n, $NAMESPACE, --context, $CONTEXT]
```

A plugin runs a command on the computer where Kubri runs, for the object under the cursor. Plugins open in a terminal over the window, so interactive commands work; press Enter or Esc to close it once the command ends. With `background: true` they run without a terminal, and with `confirm: true` Kubri shows the command and asks first. Plugins with `dangerous: true` are left out when Kubri is read-only.

```yaml
    inputs:
      - { name: lines, label: Lines, type: number, default: 100, required: true }
      - { name: level, type: dropdown, options: [info, debug] }
```

`inputs` (up to five) are asked for before the plugin runs, with the command shown as you fill them in. Their values are `$INPUT_LINES`, `$INPUT_LEVEL` and so on. The types are `string`, `number`, `bool` and `dropdown`, checked as in k9s.

`scopes` names the views a plugin is offered in: resource names such as `pods`, `po` or `apps/v1/deployments`, `containers` (which runs on pods), `helm` for Helm releases, or `all`. The footer and the help (<kbd>?</kbd>) list the plugins of the current view.

The variables are those of k9s: `$NAME`, `$NAMESPACE`, `$CONTEXT`, `$CLUSTER`, `$USER`, `$KUBECONFIG`, `$FILTER`, `$RESOURCE_GROUP`, `$RESOURCE_VERSION`, `$RESOURCE_NAME`, and for pods `$POD` and `$CONTAINER` (the default container). `$COL-<NAME>` is a column of the selected row, like `$COL-STATUS`, and `${COL-...}` takes names with other characters. Variables Kubri does not know come from the environment, or stay as written. Values go into the arguments as they are, so a plugin that hands them to a shell (`bash -c "..."`) should quote them: a column's text can hold anything.

## Keys

Shortcuts are a letter, a digit or F1 to F12, with `Ctrl-`, `Shift-` or `Alt-` in front. On a Mac, Cmd works like Ctrl. Like in k9s, a key Kubri uses in a view stays Kubri's there, unless the hotkey or plugin sets `override: true`; the `:config` page marks such keys as shared.

## Skins

Kubri takes the main colors of a skin: the background and text (`body`), the accent (`body.logoColor`), the status colors (`frame.status`) and the YAML colors (`views.yaml`). Colors set to `default` keep Kubri's own, and a skin without a background uses Kubri's dark colors, as terminal skins expect a dark terminal.

The skin comes from `K9S_SKIN`, the context's `config.yaml` or the global one, as in k9s. You can override it per context on the `:config` page or with `:skin <name>`; `:skin kubri` brings back Kubri's light and dark colors. That choice is kept in the app, not written to your k9s files.

A skin whose text has less than the 4.5:1 contrast WCAG asks for on its background, or whose status colors are hard to see, is marked **Hard to read** in the list, with the reason. Kubri still shows it as it is.

## What Kubri covers

Compared with k9s 0.51:

- **Views:** every resource type and CRD, the command palette with aliases, namespaces, labels and `@context`, Pulses, XRay, Popeye (`:popeye`), RBAC, Helm, port-forwards and benchmarks.
- **Moving around:** <kbd>Enter</kbd> drills down as above, <kbd>Shift</kbd>+<kbd>J</kbd> jumps to the owner, <kbd>o</kbd> shows a pod's node, <kbd>Shift</kbd>+<kbd>W</kbd> switches to the namespace of the row, <kbd>u</kbd> uses a namespace, and <kbd>[</kbd>, <kbd>]</kbd> and <kbd>-</kbd> go back, forward and to the last view (also <kbd>Alt</kbd>+arrows and the mouse's side buttons).
- **Lists:** filters as in k9s (a regular expression, `!` for the rows that do not match, `-f` for fuzzy names, `-l` for labels), <kbd>Ctrl</kbd>+<kbd>Z</kbd> for faulty rows only, sorting with the keys of k9s (<kbd>Shift</kbd> with <kbd>N</kbd> name, <kbd>A</kbd> age, <kbd>S</kbd> status, <kbd>P</kbd> namespace, <kbd>R</kbd> ready, <kbd>T</kbd> restarts, <kbd>I</kbd> IP, <kbd>O</kbd> node, <kbd>C</kbd> CPU, <kbd>M</kbd> memory, <kbd>X</kbd> %CPU and <kbd>Z</kbd> %memory), <kbd>Ctrl</kbd>+<kbd>F</kbd> and <kbd>Ctrl</kbd>+<kbd>B</kbd> to page, marking ranges with <kbd>Ctrl</kbd>+<kbd>Space</kbd>, <kbd>Ctrl</kbd>+<kbd>S</kbd> to save as CSV, <kbd>c</kbd> and <kbd>n</kbd> to copy names and namespaces, and %CPU and %Mem against requests, limits and node capacity.
- **Actions:** describe, YAML, edit, delete, scale, restart, labels, logs, shell, attach, port-forward, cordon, uncordon and drain; <kbd>t</kbd> runs a CronJob now and <kbd>s</kbd> suspends or resumes it; <kbd>i</kbd> sets images; <kbd>Ctrl</kbd>+<kbd>L</kbd> rolls a Deployment back to a ReplicaSet; <kbd>Shift</kbd>+<kbd>D</kbd> opens a shell on a node; <kbd>z</kbd> shows a Deployment's ReplicaSets, and on pods deletes the completed and failed ones; <kbd>x</kbd> shows a Secret's values; <kbd>u</kbd> on a ConfigMap, Secret, PVC, ServiceAccount or PriorityClass shows what uses it.
- **Logs:** <kbd>0</kbd>–<kbd>6</kbd> for the range, <kbd>t</kbd> timestamps, <kbd>w</kbd> wrap, <kbd>s</kbd> autoscroll, <kbd>m</kbd> a mark, <kbd>c</kbd> copy, <kbd>Ctrl</kbd>+<kbd>S</kbd> save, <kbd>Shift</kbd>+<kbd>C</kbd> clear.

## Not supported

- Hotkeys and plugins for keys other than letters, digits and F1 to F12, `overwriteOutput` and output pipes for plugins.
- Plugins on a single container: container plugins run on the pod's default container.
- `views.yaml`: Kubri has its own column chooser instead (**Columns** above a list, see [Creating and changing](CREATE.md)).
- The `:workloads` view, `:dir`, image vulnerability scans, copying files to and from containers, and renaming or deleting contexts.
- <kbd>Ctrl</kbd>+<kbd>K</kbd> to delete without asking: Kubri uses it for the command palette.
- `keepHistory` makes no difference: every view goes into the history.
