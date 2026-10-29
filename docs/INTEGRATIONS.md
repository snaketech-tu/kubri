# Integrations

Kubri finds the tools most clusters run and uses them without setup. Open **Integrations** in the sidebar, or `:integrations`, to see what it found, whether each tool answers, and its version.

| Tool | Found by | What Kubri does with it |
| --- | --- | --- |
| Prometheus | A Service like kube-prometheus-stack's `…-prometheus`, `prometheus-server` or `prometheus-operated` | Charts for pods, workloads, nodes and namespaces, and an hour of history in the overview. |
| Flux | The `kustomize.toolkit.fluxcd.io` or `helm.toolkit.fluxcd.io` API group | Lists of Kustomizations, HelmReleases and sources. |
| Argo CD | The `argoproj.io` Applications | The list of Applications. |
| cert-manager | The `cert-manager.io` API group | Lists of Certificates and issuers. |

Kubri reaches Prometheus through the API server's service proxy, with your kubeconfig's credentials, so nothing has to be exposed or port-forwarded. Your user needs `get` on `services/proxy` in Prometheus' namespace, which cluster admins have.

## Pointing elsewhere

**Point elsewhere** on the Integrations page sets, for the connected context only:

- A URL for Prometheus, such as Thanos, Mimir's Prometheus API or a Prometheus outside the cluster.
- A URL that names a Service, like `http://prometheus.monitoring.svc:9090`, for one Kubri does not find on its own. It is still reached through the API server.
- `off`, to turn a tool off.

The settings live in Kubri's [settings file](SETTINGS.md), per context.

## Metrics

**Metrics** (<kbd>m</kbd>) in the inspector of a pod, Deployment, StatefulSet, DaemonSet, ReplicaSet, Job, CronJob, node or namespace shows, over the last hour, 6 hours, 24 hours or 7 days:

| Chart | From | Lines |
| --- | --- | --- |
| CPU | `container_cpu_usage_seconds_total` (cAdvisor) | Per container of a pod, per pod of a workload, with the request and limit |
| Memory | `container_memory_working_set_bytes` (cAdvisor) | The same, with the request and limit |
| Container restarts | `kube_pod_container_status_restarts_total` (kube-state-metrics) | Per container or pod |
| Network | `container_network_*_bytes_total` (cAdvisor) | Received and sent |
| Requests | `http_requests_total` and the names Spring, OpenTelemetry and others use | Per status class, when the app counts requests |

A node's charts come from node-exporter when it runs, and from cAdvisor otherwise. Rollouts (a Deployment's ReplicaSets, a StatefulSet's or DaemonSet's revisions) and container restarts show as dashed lines, and hovering one names it. **Copy PromQL** copies the query behind a chart, to refine it in Prometheus or Grafana. A workload's pods are found by the names their controller gives them, so pods that are gone still count.

With Prometheus, the **Usage trend** in the overview starts with the last hour of measured use instead of only what Kubri saw since it opened.

## AI agents

Agents connected over [MCP](AGENTS.md) get two read-only tools: `get_metrics`, the same charts as numbers, and `query_prometheus`, any PromQL over the last hours.
