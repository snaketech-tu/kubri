# Integrations

Kubri finds the tools most clusters run and uses them without setup. Open **Integrations** in the sidebar, or `:integrations`, to see what it found, whether each tool answers, and its version.

| Tool | Found by | What Kubri does with it |
| --- | --- | --- |
| Prometheus | A Service like kube-prometheus-stack's `…-prometheus`, `prometheus-server` or `prometheus-operated` | Charts for pods, workloads, nodes and namespaces, and an hour of history in the overview. |
| Grafana | A Service like `…-grafana` or `grafana` | The logs of the object you pick in Grafana's Explore, and links to the dashboards that show it, opened already filtered. |
| Loki | A Service like `loki`, `loki-gateway` or `loki-read` | Log history beyond a pod's lifetime, in the log workspace. |
| Flux | The `kustomize.toolkit.fluxcd.io` or `helm.toolkit.fluxcd.io` API group | State, revision and managed objects of Kustomizations, HelmReleases and sources; reconcile, suspend and resume. |
| Argo CD | The `argoproj.io` Applications | Sync state, health and managed objects of Applications; sync, refresh and auto-sync. |
| cert-manager | The `cert-manager.io` API group | Expiry and readiness of Certificates in their list, the Overview and Checks; renew now. |

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

## Right-sizing

**Right-sizing** (`:rightsizing`, in the sidebar) compares what each container of the namespace's Deployments, StatefulSets and DaemonSets requests with what it uses:

| Use | From |
| --- | --- |
| CPU | The 95th percentile of the 5-minute rate over 7 days, from Prometheus |
| Memory | The highest working set over 7 days, from Prometheus |

Without Prometheus it falls back to the metrics server, a single measurement of now, and says so; it then only suggests lowering requests that are far (4×) above use.

Suggestions come with a reason, the most urgent first:

- **Near limit**: use is at 90% of the limit or more. The container will be throttled (CPU) or killed (memory).
- **Under-requested**: use is above the request, so the scheduler places the pod on a node that may not have room.
- **No request**: the container has none, so the scheduler counts it as free.
- **Over-requested**: the request is at least twice the suggestion, and by at least 50m CPU or 64Mi memory.

The suggested request is use with 20% headroom, the suggested limit use with 50%, rounded up to 5m or 16Mi. The page sums what lowering the over-requested ones would free. **Review…** opens the change with the dry-run diff of the workload's template, the matching `kubectl set resources` command and, when Flux or Argo CD manage the workload, what they will do with it. Setting the values restarts the workload's pods, as any template change does.

## Grafana

Give Grafana's address under **Point elsewhere > Grafana > URL** on the Integrations page, like `https://grafana.example.com`, or let Kubri find Grafana in the cluster. Kubri fills in the rest from the object you pick.

### Logs in Grafana

**Logs in Grafana** in the menu of a namespace, pod, workload or Service opens its logs of the last hour in Grafana's Explore, with the Loki query already set:

| Picked | Query |
| --- | --- |
| A namespace | `{namespace="shop"}` |
| A pod | `{namespace="shop", pod="web-1"}` |
| A Deployment, StatefulSet, DaemonSet, ReplicaSet, Job or CronJob | Its pods by the names its controller gives them, like `{namespace="shop", pod=~"web-[a-z0-9]{1,10}-[a-z0-9]{5}"}` |
| A Service | The pods it selects right now, by name |
| Anything else in a namespace, like a ConfigMap | Its namespace |

The queries use the `namespace`, `pod` and `container` labels that Promtail, Grafana Alloy and most log collectors set. Where yours names them differently, such as `exported_namespace`, see [Other label names](#other-label-names). Kubri asks Grafana which Loki data source it has and takes the default one, else the one named `Loki`. When Grafana needs a login or a client certificate and you gave no token, Kubri cannot ask, and Grafana opens its own default data source; give the name or UID of the Loki data source under **Loki data source** then. Your browser opens the page, so a login or a client certificate there works as usual.

### Other label names

Some log collectors give logs other label names, such as `exported_namespace` instead of `namespace`. **Templates…** next to **Log label names** on the integrations page names them once, as a template with a name and the label for the namespace, the pod and the container; a field left empty keeps the usual name. Templates are kept for all contexts, and each context picks one under **Log label names**. Logs in Grafana and the log history from Loki in the log workspace then use those names.

### Dashboards

**Grafana…** in the menu of every object lists, in a dialog, its logs in Explore, the dashboards that show it, and **Search Grafana for** its name, each with **Copy** for its whole address. **Metrics** lists the same above the charts, and the Integrations page lists them for the namespace picked in the sidebar.

A dashboard fits when it has a variable Kubri can fill, like `pod`, `workload`, `deployment` or `node`, with the namespace too, and opens with those variables set. Dashboards come from Grafana's own list and from the ConfigMaps labeled `grafana_dashboard` that kube-prometheus-stack and most charts ship, so they show even when Grafana needs a login. For a Grafana given by URL that needs a login, a service account token with the Viewer role under **Token** lets Kubri read its list and its data sources. Kubri reads them again after five minutes.

## Loki

In the log workspace (<kbd>l</kbd>), **Loki history** in the source menu shows what Loki kept over the last 15 minutes to 7 days, including pods that no longer exist. The LogQL field starts with the object's own query (by pod name for pods and workloads, by the names of the pods it selects for a Service, and the container when one is picked) and runs any other with **Run**. The lines take the same search, levels, fields, table and export as live logs. A query returns at most 5000 lines, the newest; a note at the top says when there were more.

## Flux and Argo CD

Lists of Kustomizations, HelmReleases and Flux sources get **Revision** and **Suspended** columns; Applications get **Revision** and **Last sync**. **Columns** hides them or puts them back.

The overview of a Flux object or an Application says whether it is ready, synced and healthy, why not, and its revision, source and last reconcile or sync. Below that are the objects it manages, from a Kustomization's inventory or an Application's resource list, with those out of sync or unhealthy first; each opens with a click.

| Kind | Actions |
| --- | --- |
| Flux objects | **Reconcile** (<kbd>r</kbd>), like `flux reconcile`; **Suspend** and **Resume** (<kbd>s</kbd>), like `flux suspend` and `flux resume` |
| Argo CD Applications | **Sync** (<kbd>s</kbd>), like `argocd app sync`; **Refresh** (<kbd>r</kbd>), to compare with Git again; **Auto-sync** (<kbd>t</kbd>), to turn automated sync off and on again as it was |

The actions change the objects the controllers watch, the way their CLIs do, so they need `patch` on them and no Flux or Argo CD login.

An object that Flux or Argo CD manages says so in its overview, when it is edited and in the dialogs that change it: which Kustomization, HelmRelease or Application manages it, and whether changes made by hand stay or are put back, at the next reconcile or right away with Argo CD's self-heal. Kubri finds the manager by the labels Flux sets and by Argo CD's tracking annotation or its `app.kubernetes.io/instance` label.

## cert-manager

The Certificate list gets **Expires** and **Renews** columns, as times from now. A Certificate's overview shows whether it is ready, when it expires and renews, its issuer, Secret and names.

Kubri judges a Certificate the way it judges a Deployment:

- **Failed** when it is expired, not ready, or in the last week (or sixth) of its lifetime.
- **Warning** in the last three weeks (or third) of its lifetime, past the point cert-manager renews by default, or when it was never issued.

Short-lived certificates are judged by their lifetime, so a 24-hour certificate is fine with 20 hours left. The Overview (`:pulses`) counts Certificates under Resource health and lists the ones that need a look, failed first, then the soonest to expire. The sanitizer has a Certificate section with the same findings.

**Renew now** (<kbd>r</kbd>) has cert-manager issue a Certificate again, as `cmctl renew` does, by adding an `Issuing` condition. It needs `patch` on `certificates/status`.

## AI agents

Agents connected over [MCP](AGENTS.md) get two read-only tools: `get_metrics`, the same charts as numbers, and `query_prometheus`, any PromQL over the last hours.
