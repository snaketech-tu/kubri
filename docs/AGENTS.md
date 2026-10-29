# AI agents (MCP)

Kubri is an [MCP](https://modelcontextprotocol.io) server. AI agents such as Claude Code, Claude Desktop, Cursor or an IDE assistant can look at your clusters through it, with your own permissions and Kubri's guard rails. Open **Settings → AI agents** (or `:agents`) for the commands and configuration to paste into an agent.

## Two ways to connect

| | The agent starts Kubri (stdio) | Through the running Kubri (HTTP) |
| --- | --- | --- |
| Set up | The agent runs Kubri's program with `--mcp`: `kubri --mcp` for the desktop app, `kubri-server --mcp` for the server. | Turn on **Serve MCP on this computer**. The desktop app listens on `http://127.0.0.1:7879/mcp` (the port can be changed), the web server on its own address at `/mcp`. |
| Sign-in | None: it runs as you, on your computer. | A token, sent as `Authorization: Bearer …`. **New token** makes another and locks out the old one. |
| Changes | Never: no Kubri window can confirm them. | Only on contexts that allow them, each one confirmed in the Kubri window. |

Settings → AI agents shows the exact command for Claude Code (`claude mcp add …`) and the JSON for other clients, with the right path and token.

## Tools

| Tool | What it does |
| --- | --- |
| `list_contexts` | The contexts in the kubeconfig, their names and labels in Kubri, and whether agents may change them. |
| `list_resource_types` | The resource types the cluster serves, custom resources included. |
| `list_objects` | A list with the columns of `kubectl get`, by namespace, label or field selector, paged. |
| `get_object` | One object as YAML. |
| `get_events` | The events about one object. |
| `get_logs` | The last lines of a pod's log, or of every pod of a workload or Service. |
| `diagnose_pod` | Why a pod fails: crashing or waiting containers, exit codes, scheduling problems, warning events and the crashed container's last log lines. |
| `check_problems` | The sanitizer's report with score and grade. |
| `show_tree` | XRay: workloads down to their pods, containers and what they use. |
| `can_i` | Whether you may do something, like `kubectl auth can-i`. |
| `cluster_health` | Pulses: health per kind, usage and recent warnings. |
| `get_metrics` | CPU, memory, restarts, network and request rates of a pod, workload, node or namespace over the last hours, from Prometheus. |
| `query_prometheus` | Runs a PromQL query over the last hours; min, average, max and latest per series. |
| `what_changed` | What Kubri recorded in the last hours: rollouts, image changes, scaling, ConfigMap and Secret keys, nodes, deletions and warnings (see [CHANGES.md](CHANGES.md)). |
| `scale`, `restart`, `set_image`, `delete_object` | Changes, only over HTTP and only where allowed (see below). |

Resources can be named as with kubectl: `pods`, `po`, `deploy`, `deployments.apps` or a kind. Each tool takes an optional `context`; without it, tools work on your kubeconfig's current context. The agent has a Kubri of its own, so switching contexts there never changes the context you have open in Kubri.

## What agents may not do

- **Change a cluster, by default.** A context allows changes only when **AI agents may change it** is on in its settings. Production contexts never allow it, nor do read-only ones, locked ones, or any context when Kubri runs with `--read-only` or the k9s `readOnly` setting. These rules are enforced in Kubri's core, not only by hiding the tools.
- **Change anything without you.** Each change waits for **Allow** in the Kubri window, which shows what will happen and the equivalent `kubectl` command. With no window open, or no answer within two minutes, the change is refused.
- **Read Secret values.** Agents see a Secret's keys and sizes. **Agents may read Secret values** in the settings lifts this.
- **Do more than you may.** Every call goes to the cluster with your credentials, so RBAC applies as it does to you.
- **Flood their context.** Lists default to 100 rows and logs to 100 lines per container. Any result is cut after about 60 KB, with a note on how to ask for less.

Every call, from either way of connecting, is logged with its arguments, context and outcome in `agent-calls.jsonl` in Kubri's config folder (see [Settings](SETTINGS.md#where-the-file-is)). The latest calls are listed under Settings → AI agents.
