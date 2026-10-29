# Snapshots

A snapshot saves a namespace or a whole cluster to one file, as Kubri shows it. Anyone with Kubri can open the file on another computer, without access to the cluster, and look around with the normal pages: lists, details, YAML, events, logs, Overview, XRay, the graph, the sanitizer, RBAC and Helm. It is for "send me what you see", for an incident review, or for asking for help with a cluster you cannot share.

## Saving one

`:snapshot` opens the dialog for the namespace picked in the top bar, `:snapshot shop` for `shop`, and `:snapshot all` for the whole cluster. **Snapshots…** at the bottom of the context switcher opens it too. The desktop app saves the file in the Downloads folder; in the browser it downloads, like `prod-shop-20261029-1403.kubri`.

A snapshot holds:

| What | How much |
| --- | --- |
| Objects of every type you may list, CRDs included, with the columns of their lists | Up to 5,000 per type |
| For a namespace: the namespace, nodes, volumes, storage, ingress, priority and runtime classes, CRDs and cluster roles and their bindings, which its objects refer to | All |
| Events | The newest 2,000 |
| Recent log lines of containers that have run | The last 500 lines, at most 256 KiB, of up to 300 containers |
| Metrics, where metrics-server runs | As they were |

It does not hold:

- **Secret values.** Secrets keep their keys, labels and annotations; their values are empty, and the copy that `kubectl apply` keeps in an annotation is removed.
- **Helm values and manifests.** Releases keep their name, revision, status, times and chart, so the Helm page lists them and their history, but values, notes and manifests, which can hold secrets, are left out.
- Managed fields, logs of earlier container instances, and types you may not list. What was left out and why is listed under **left out** in the snapshot's banner.

ConfigMaps are kept as they are: check that they hold nothing you would not send.

The file is gzipped JSON; `zcat file.kubri | jq .info` shows what it holds.

## Opening one

**Open a file…** in the same dialog opens a snapshot in place of the cluster. `kubri-server --snapshot file.kubri` opens one at start, for a machine without a kubeconfig.

While a snapshot is open:

- A banner names the context it came from, the namespace, and when it was taken, and the context switcher says **Snapshot**. Nothing claims to be live.
- It is read-only: actions that change something are not offered, and port-forwards, shells and the debug tools have nothing to connect to.
- Ages, "recent" events on the Overview and the sanitizer count from when the snapshot was taken, not from now.
- Logs show the saved lines. **Compare** in an object's menu compares it with the same object in a live context of your kubeconfig, which shows what changed since.
- What changed and the integrations (Prometheus, Grafana, Loki) have nothing to show.

**Leave snapshot** in the banner, or picking any context, goes back to a live cluster.
