# Settings and guard rails

Kubri keeps its own settings in one file. The kubeconfig and the k9s configuration are never written to. Open them with **Settings** in the sidebar, `:settings` in the command palette, or the context settings button beside the context switcher.

## Where the file is

| System | Path |
| --- | --- |
| Linux | `$XDG_CONFIG_HOME/kubri/settings.json`, usually `~/.config/kubri/settings.json` |
| macOS | `~/Library/Application Support/kubri/settings.json` |
| Windows | `%LOCALAPPDATA%\kubri\settings.json` |

`KUBRI_CONFIG_DIR` puts it in another folder, which is useful for the Docker image: mount a volume there to keep the settings. If the file cannot be read, Kubri says so in a banner and does not overwrite it.

## Per context

| Setting | What it does |
| --- | --- |
| Display name | Shown instead of the context's name in the switcher, the header and the status bar. |
| Label | A word such as `Staging` beside the name. Kubri never guesses it from the name. |
| Color | Tints the header and marks the context in the switcher. |
| Production | Shows a banner, and dangerous actions, such as deleting, draining or uninstalling a Helm release, ask you to type the object's name (or the context's, for bulk actions) first. |
| Read-only | Kubri never changes this cluster until you turn it off. |
| Locked | Starts read-only. **Unlock** in the header allows changes for 15 minutes, then the context locks again. |
| Namespace | The namespace shown after connecting, instead of all namespaces. |
| AI agents may change it | Agents connected over MCP may scale, restart, set images and delete here, each change allowed by you in Kubri. Never on production contexts. See [AI agents](AGENTS.md). |
| Integrations | Where Prometheus, Grafana and Loki are, when Kubri should not find them itself, and a Grafana token. Set on the Integrations page. See [Integrations](INTEGRATIONS.md). |

Read-only and locked contexts are enforced in the core, not only hidden in the interface, so plugins and the API refuse changes too. Starting Kubri with `--read-only` (or `KUBRI_READ_ONLY=true`) and `readOnly` in the k9s configuration still apply on top.

## Permissions

Kubri asks the cluster which actions you may run, with the same `SelfSubjectAccessReview` that `kubectl auth can-i` uses. Actions you may not run stay visible but are marked. Choosing one says which permission is missing, such as `delete` on `pods` in `shop`, shows a Role and RoleBinding that would grant it to send to an admin, and **Who may do this?** opens the Access view filtered to that permission. When you may not list all namespaces, lists show the namespaces you may read instead of failing.

## Dry-run diffs

Before a change, Kubri asks the API server to run it as a dry run (`dryRun=All`) and shows what would change, without status, `managedFields` and `resourceVersion`, like `kubectl diff`. YAML edits always show the diff. **Dry-run diffs** under Settings decides it for setting images, labels and annotations (on by default), and for scaling and restarting (off by default).

Every action can also be copied as the `kubectl` command that does the same: in its dialog under **Same as kubectl**, or with **Copy as kubectl…** in the row menu.

## The YAML editor

The editor loads the kind's schema from the API server (`/openapi/v3`), so it knows custom resources as well as built-in ones:

- **Field help** under the editor explains the field at the cursor, like `kubectl explain`: its type, its description and the fields it can hold.
- Typing a key offers the fields allowed there, required ones first. <kbd>Ctrl</kbd>+<kbd>Space</kbd> opens the list.
- Field names the schema does not know are underlined. Free-form parts, such as labels or the open parts of a custom resource, are left alone.

While you edit, Kubri checks the object on the server every 5 seconds. When someone else changes it, **Compare** shows their changes and yours. **Apply mine to theirs** then puts your changes on top of the latest version. Where you both changed the same field, yours win and the field is named. Nothing is saved until you review the result and save.
