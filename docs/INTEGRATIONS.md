# Integrations

Kubri finds the tools most clusters run and uses them without setup. Open **Integrations** in the sidebar, or `:integrations`, to see what it found, whether each tool answers, and its version.

| Tool | Found by | What Kubri does with it |
| --- | --- | --- |
| Prometheus | A Service like kube-prometheus-stack's `…-prometheus`, `prometheus-server` or `prometheus-operated` | Charts for pods, workloads, nodes and namespaces, and an hour of history in the overview. |
| Grafana | A Service like `…-grafana` or `grafana` | Links from objects to the dashboards that show them, opened already filtered. |
| Loki | A Service like `loki`, `loki-gateway` or `loki-read` | Log history beyond a pod's lifetime, in the log workspace. |
| Flux | The `kustomize.toolkit.fluxcd.io` or `helm.toolkit.fluxcd.io` API group | State, revision and managed objects of Kustomizations, HelmReleases and sources; reconcile, suspend and resume. |
| Argo CD | The `argoproj.io` Applications | Sync state, health and managed objects of Applications; sync, refresh and auto-sync. |
| cert-manager | The `cert-manager.io` API group | Lists of Certificates and issuers. |

Kubri reaches Prometheus, Grafana and Loki through the API server's service proxy, with your kubeconfig's credentials, so nothing has to be exposed or port-forwarded. Your user needs `get` on `services/proxy` in their namespace, which cluster admins have. Only opening Grafana in the browser needs a port-forward, which Kubri starts the first time and keeps under Port-forwards.

## Pointing elsewhere

**Point elsewhere** on the Integrations page sets, for the connected context only:

- A URL for Prometheus, such as Thanos, Mimir's Prometheus API or a Prometheus outside the cluster, and the same for Grafana and Loki.
- A Grafana token, a service account token with the Viewer role, for a Grafana given by URL that needs a login.
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

## Grafana dashboards

**Metrics** lists, above the charts, the Grafana dashboards that show the object: those with a variable Kubri can fill, like `pod`, `workload`, `deployment` or `node`, with the namespace too. Each opens in Grafana with those variables set. The Integrations page lists the dashboards for the namespace picked in the sidebar and opens Grafana itself.

Dashboards come from Grafana's own list and from the ConfigMaps labeled `grafana_dashboard` that kube-prometheus-stack and most charts ship, so they show even when Grafana needs a login. Kubri reads them again after five minutes.

## Loki

In the log workspace (<kbd>l</kbd>), **Loki history** in the source menu shows what Loki kept over the last 15 minutes to 7 days, including pods that no longer exist. The LogQL field starts with the object's own query (by pod name for pods and workloads, by selector labels for a Service, and the container when one is picked) and runs any other with **Run**. The lines take the same search, levels, fields, table and export as live logs. A query returns at most 5000 lines, the newest; a note at the top says when there were more.

## Flux and Argo CD

Lists of Kustomizations, HelmReleases and Flux sources get **Revision** and **Suspended** columns; Applications get **Revision** and **Last sync**. **Columns** hides them or puts them back.

The overview of a Flux object or an Application says whether it is ready, synced and healthy, why not, and its revision, source and last reconcile or sync. Below that are the objects it manages, from a Kustomization's inventory or an Application's resource list, with those out of sync or unhealthy first; each opens with a click.

| Kind | Actions |
| --- | --- |
| Flux objects | **Reconcile** (<kbd>r</kbd>), like `flux reconcile`; **Suspend** and **Resume** (<kbd>s</kbd>), like `flux suspend` and `flux resume` |
| Argo CD Applications | **Sync** (<kbd>s</kbd>), like `argocd app sync`; **Refresh** (<kbd>r</kbd>), to compare with Git again; **Auto-sync** (<kbd>t</kbd>), to turn automated sync off and on again as it was |

The actions change the objects the controllers watch, the way their CLIs do, so they need `patch` on them and no Flux or Argo CD login.

An object that Flux or Argo CD manages says so in its overview, when it is edited and in the dialogs that change it: which Kustomization, HelmRelease or Application manages it, and whether changes made by hand stay or are put back, at the next reconcile or right away with Argo CD's self-heal. Kubri finds the manager by the labels Flux sets and by Argo CD's tracking annotation or its `app.kubernetes.io/instance` label.

## AI agents

Agents connected over [MCP](AGENTS.md) get two read-only tools: `get_metrics`, the same charts as numbers, and `query_prometheus`, any PromQL over the last hours.
