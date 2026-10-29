# Logs and debugging

From "this pod is broken" to the cause and a fix without leaving Kubri: logs you can filter by field, a workspace that keeps them open while you look elsewhere, debug containers and node shells, and a fix offered with each diagnosis.

## Logs

Press <kbd>l</kbd> on a pod, a workload or a Service, or <kbd>p</kbd> for the previous instance of a pod's container. A workload's log merges all its pods and containers, each in its own color. Pods that start during a rollout join the log, and a line across the log says when one joined or when a container's log ended. The **Pods** chips above the log hide or show one pod.

| What | How |
| --- | --- |
| Levels | Kubri recognizes the level of JSON lines (`level`, `severity`, `lvl`), logfmt lines (`level=warn`) and plain text (`ERROR`, `[warn]`, klog's `E1029`, stack traces). **All levels** keeps only lines at or above a level. |
| Search | <kbd>/</kbd>. Plain text, a `/regex/`, or field terms such as `level>=warn status>=500 path~/api`. Terms use `=`, `!=`, `>`, `>=`, `<`, `<=` and `~` (contains); numbers compare as numbers. `level`, `pod` and `container` work on every line, other names on the fields of JSON and logfmt lines, with dots for nested fields. |
| Matches | <kbd>n</kbd> and <kbd>N</kbd> (or <kbd>Enter</kbd> in the search) step through matches. Turn off **Only matches** to see every line with the matches highlighted. |
| Table | **Table** shows JSON and logfmt lines as columns: time, level, source and the most frequent fields. **Columns** picks others. The `{…}` button beside a line shows its fields pretty-printed. |
| Fold | Repeated lines show once with `× 312`, and stack traces fold under their first line; click to unfold. |
| Rate strip | Lines per time slot above the log, with errors and warnings in their colors. Click a bar to jump to that time. |
| Range | <kbd>0</kbd> to <kbd>6</kbd>: the last lines, all, or the last 1, 5, 15 or 30 minutes or hour. |
| Copy and save | <kbd>c</kbd> copies and <kbd>Ctrl</kbd>+<kbd>S</kbd> saves exactly what is shown, folds and filters included. |
| Explain | <kbd>x</kbd> asks your model about the selected lines, or the last 200 shown. See [Explain](EXPLAIN.md). |

<kbd>t</kbd>, <kbd>w</kbd>, <kbd>s</kbd> and <kbd>m</kbd> switch timestamps, wrapping and autoscroll and draw a mark, as in k9s.

## The workspace

Logs and shells in the inspector stop when you open something else. To keep them, use **Move to the workspace** (the arrow at the top right of the inspector) or **Logs in workspace** and **Shell in workspace** in an object's menu. They open as tabs in a workspace below the page and keep running while you browse.

- Tabs: one per log or shell. Opening the same log again shows its tab; every shell gets its own. A middle click closes a tab.
- **Side by side** shows two tabs at once, such as two pods' logs or a log and a shell. The next tab you pick replaces the one you used last.
- Dock it **at the bottom** or **at the right**, and drag its edge (or focus the edge and use the arrow keys) to resize it. Kubri remembers both.
- **Minimize** leaves a task bar of tabs; what runs in them keeps running.
- The log keys work in the tab you clicked last; a click on the page gives them back to the list.
- Tabs belong to the context they were opened in, and close when you switch contexts.

## Debug containers and copies

**Debug…** (<kbd>Shift</kbd>+<kbd>D</kbd>) on a pod offers two ways in:

- **Debug container**: a container with your tools, `busybox` unless you pick another image, joins the pod like `kubectl debug -it --target`. It sees the processes of the container you pick, so it works for images without a shell, such as distroless ones. Kubri remembers the image. Debug containers stay in the pod until the pod is replaced.
- **Debug copy**: a copy of the pod in which the container runs `sleep infinity` instead of its command, with probes removed, for crash loops. The copy has no labels, so Services send it no traffic, and it stays until you delete it.

Either way the shell opens in the inspector. The likely-problem summary of a crash-looping pod offers **Debug copy…** directly.

## Node shell

**Node shell…** (<kbd>Shift</kbd>+<kbd>D</kbd> on a node) starts a privileged pod on the node that shares its processes, network and files, and opens a shell in them (with `nsenter`, or `chroot /host` where that is not allowed). Anything you do there acts on the node itself, which the dialog says before it starts. The pod is removed when the shell ends. Pick a namespace that allows privileged pods. Like all shells, it is not offered in read-only mode.

## Fixes from the likely problem

The summary at the top of a failing pod names the likely problem, and now offers what to do about it:

| Problem | What Kubri offers |
| --- | --- |
| Crash loop | The previous container's logs, the current logs, and a debug copy. |
| OOMKilled | **Raise memory limit…** on the Deployment, StatefulSet or DaemonSet that owns the pod, with a suggested limit and the change as a dry-run diff before it is applied. |
| Image pull errors | The image, its registry and the pull secrets, and whether each secret exists. |
| Pending | **Why not each node?** lists per node what rules it out: not ready, cordoned, taints, the node selector, required node affinity, CPU, memory and the pod count. Pod affinity, topology spread and volumes are left to the scheduler's own message. |

A Service whose pods do not get traffic gets a summary too: no pod matches its selector (with the pods that nearly match and the labels that differ), no matching pod is ready, or the pods lack a named target port.
