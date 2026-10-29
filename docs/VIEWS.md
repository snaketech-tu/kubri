# Views for custom resources

A view is a YAML file that teaches Kubri a kind, usually a CRD, without code. It can add:

- **Columns** to the kind's list, read from each object with JSONPath.
- A **health state**, shown as a Health column, counted under **Needs attention**, and drawn in the topology graph.
- **Sections** on the Overview tab of an object's details.
- **Actions** in the object's menu, which run a command like a k9s plugin.
- **Relations** to other objects, drawn as lines in the topology graph.

Views run no code, so they work the same in the desktop app and the browser.

## Where they live

Kubri ships views for Flux, Argo CD and cert-manager. Your own go in the `views` folder next to Kubri's [settings file](SETTINGS.md):

| System | Folder |
| --- | --- |
| Linux | `$XDG_CONFIG_HOME/kubri/views`, usually `~/.config/kubri/views` |
| macOS | `~/Library/Application Support/kubri/views` |
| Windows | `%LOCALAPPDATA%\kubri\views` |

With `KUBRI_CONFIG_DIR` set, it is `$KUBRI_CONFIG_DIR/views`. Every `.yaml` or `.yml` file there is read, in name order, and a file can hold several views separated by `---`. A view of the same group and kind as an earlier one replaces it, so your file can change a shipped view.

Kubri reads the files again when it connects to a context, and when you press **Reload** on the settings page. `:views` opens the **Views** section there, which lists every view, its file, and the files or views that could not be read and why. A view with a mistake is left out; the others still work.

## An example

```yaml
group: example.com
kind: Gadget
columns:
  - { name: Color, path: .spec.color }
  - { name: Size, path: .spec.size }
health:
  - { path: .spec.size, matches: "^[0-2]$", state: warning, message: Too small }
  - { path: '.status.conditions[?(@.type=="Ready")].status', equals: "False", state: failed, message: '.status.conditions[?(@.type=="Ready")].message' }
  - { path: .spec.color, exists: true, state: ok }
details:
  - title: Build
    fields:
      - { name: Color, path: .spec.color }
      - { name: Size, path: .spec.size }
actions:
  - name: Paint
    shortcut: Shift-P
    command: kubectl
    args: [annotate, gadget, $NAME, -n, $NAMESPACE, --context, $CONTEXT, painted=yes]
    confirm: true
    dangerous: true
relations:
  - { kind: ConfigMap, name: .spec.configMapRef.name }
```

The shipped views in [crates/kubri-core/views](../crates/kubri-core/views) are more examples.

## The fields

| Field | Meaning |
| --- | --- |
| `group` | The API group, like `cert-manager.io`; leave it out for the core group. |
| `kind` | The kind, like `Certificate`. |
| `columns` | `name` and `path` of each column, added after the server's. |
| `health` | Rules for the health state; see below. |
| `details` | Sections with a `title` and `fields` (`name` and `path`), shown on the Overview tab. Fields whose path finds nothing are left out, and so are sections left empty. |
| `actions` | Commands for the object's menu; see below. |
| `relations` | Objects this one refers to; see below. |

Unknown fields are an error, so a typo does not go unnoticed.

### Paths

Paths are JSONPath as `kubectl get -o custom-columns` takes it: `.spec.nodeName`, `.spec.containers[*].image`, `.status.conditions[0].type`, `.metadata.labels['app.kubernetes.io/name']`, and filters on a field of a list's elements, `[?(@.type=="Ready")]` or `[?(@.type!="Ready")]`. Several values are joined with commas.

Columns are the list's own: the **Columns** button can hide, move or add to them, and **Reset** goes back to the view's.

### Health

The rules are tried in order and the first that matches decides; when none matches, the object has no health state. A rule has a `path`, a `state` (`ok`, `warning` or `failed`), an optional `message`, and one test:

| Test | Matches when |
| --- | --- |
| `equals: "True"` | some value at the path is this text |
| `notEquals: "True"` | the path finds values and none is this text |
| `matches: "^(Degraded\|Missing)$"` | some value matches this regular expression |
| `exists: true` or `false` | the path finds something, or nothing |
| none | the path finds something |

Values are compared as text: `true`, `3` and `"True"` are written as they print. A `message` that starts with `.` is a path, read from the object; otherwise it is the text itself. The Health column shows `Healthy`, or the state and the message, like `Failed: Secret does not exist`.

Flux, Argo CD and cert-manager objects keep their own status card on the Overview tab, so their views' health shows only in the list and the graph.

### Actions

An action is a [k9s plugin](K9S.md#plugins) for this kind: it takes `name`, `shortcut` (like `Shift-P`), `description`, `command`, `args`, `background`, `confirm`, `dangerous` and `inputs`, with the same meaning and the same variables (`$NAME`, `$NAMESPACE`, `$CONTEXT`, `$COL-HEALTH` and the others). It shows in the object's menu and in the footer as `Gadget: Paint`. Actions with `dangerous: true` are left out while Kubri is read-only.

Actions run commands on the computer where Kubri runs. In the browser that is the machine of the web server, as for k9s plugins; see [Running Kubri in the browser](WEB.md).

### Relations

A relation has a `kind`, a `name` path and an optional `namespace` path; without one, the other object is in the same namespace. `kind` is a kind like `Secret`, or a path to one, like `.spec.sourceRef.kind`. The topology graph draws a "uses" line from the object to each object it names that the graph shows. Objects of a view's kind join the graph when they have such a line or a health state other than ok.
