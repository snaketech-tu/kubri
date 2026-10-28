# Using your k9s configuration

Kubri reads the aliases, hotkeys, plugins and skins you set up for k9s, from the same files, so they work in Kubri without changes. `:config` in the command palette shows what was found, which files could not be read and why, and lets you pick a skin.

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
| `config.yaml` | `k9s.ui.skin` names the skin. |
| `clusters/<cluster>/<context>/` | In the data directory: aliases, hotkeys, plugins and `config.yaml` (with `k9s.skin`) for one context, which win over the global ones. |

Kubri reads the files again when you switch contexts or press Reload on the `:config` page.

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

A plugin runs a command on the computer where Kubri runs, for the object under the cursor. Plugins open in a terminal over the window, so interactive commands work; press Enter or Esc to close it once the command ends. With `background: true` they run without a terminal, and with `confirm: true` Kubri shows the command and asks first.

`scopes` names the views a plugin is offered in: resource names such as `pods`, `po` or `apps/v1/deployments`, `containers` (which runs on pods), `helm` for Helm releases, or `all`. The footer and the help (<kbd>?</kbd>) list the plugins of the current view.

The variables are those of k9s: `$NAME`, `$NAMESPACE`, `$CONTEXT`, `$CLUSTER`, `$USER`, `$KUBECONFIG`, `$FILTER`, `$RESOURCE_GROUP`, `$RESOURCE_VERSION`, `$RESOURCE_NAME`, and for pods `$POD` and `$CONTAINER` (the default container). `$COL-<NAME>` is a column of the selected row, like `$COL-STATUS`, and `${COL-...}` takes names with other characters. Variables Kubri does not know come from the environment, or stay as written. Values go into the arguments as they are, so a plugin that hands them to a shell (`bash -c "..."`) should quote them: a column's text can hold anything.

## Keys

Shortcuts are a letter, a digit or F1 to F12, with `Ctrl-`, `Shift-` or `Alt-` in front. On a Mac, Cmd works like Ctrl. Like in k9s, a key Kubri uses in a view stays Kubri's there, unless the hotkey or plugin sets `override: true`; the `:config` page marks such keys as shared.

## Skins

Kubri takes the main colors of a skin: the background and text (`body`), the accent (`body.logoColor`), the status colors (`frame.status`) and the YAML colors (`views.yaml`). Colors set to `default` keep Kubri's own, and a skin without a background uses Kubri's dark colors, as terminal skins expect a dark terminal.

The skin comes from `K9S_SKIN`, the context's `config.yaml` or the global one, as in k9s. You can override it per context on the `:config` page or with `:skin <name>`; `:skin kubri` brings back Kubri's light and dark colors. That choice is kept in the app, not written to your k9s files.

## Not supported yet

- Hotkeys and plugins for keys other than letters, digits and F1 to F12.
- `keepHistory`, as Kubri has no command history yet, and `overwriteOutput` and `inputs` for plugins.
- Plugins on a single container: container plugins run on the pod's default container.
