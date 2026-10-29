# What changed

While it runs, Kubri records what changes in the context it is connected to. **What changed** (`:changes`, in the sidebar under Explore) shows it as a timeline, so "it worked an hour ago" has an answer.

## What is recorded

Kubri watches these kinds and writes a line for each change that matters:

| Kind | Recorded |
| --- | --- |
| Deployments, StatefulSets, DaemonSets, CronJobs | Image changes (`app: nginx:1.25 → nginx:1.27`), restarts (`kubectl rollout restart`), other pod template changes, scaling, other spec changes, labels and annotations |
| ConfigMaps and Secrets | Which keys were changed, added or removed, never the values |
| Services, Ingresses, HorizontalPodAutoscalers | Spec changes |
| Nodes | Ready or not, cordoned and uncordoned, pressure conditions, taints, kubelet version |
| Events | Warnings, once each, and again every ten minutes while they keep happening |

Every kind also records when its objects are created and deleted. Status changes, `resourceVersion`, managed fields and the copy in `last-applied-configuration` are not changes. Kinds your user may not watch are named above the timeline.

Each change except a warning keeps the object before and after it, so **Diff** shows exactly what changed. ConfigMap and Secret values are replaced by their size and a short fingerprint, made with a key of this database, which changes when the value does: enough to see that `password` changed, not what it is.

## Honest about gaps

Kubri records only while a Kubri window or `kubri-server` is connected to the context. The timeline shows the times it was not watching as striped lines. When it connects again, it compares the cluster with what it knew:

- objects changed or deleted meanwhile get an entry marked **Happened while Kubri was not watching**, at the time Kubri found it;
- objects created meanwhile get an entry at the time they were created;
- warnings that happened meanwhile are still recorded while Kubernetes keeps their events, an hour by default.

Two Kubri windows connected to the same context do not record it twice: the first one records, the other takes over when it stops.

## Using it

- The timeline covers the namespace picked in the top bar, or all namespaces. Pick the range (the last hour up to everything kept), the kinds of change, or type part of a name or of what changed.
- **What changed** in an object's menu shows that object's changes only.
- `:changes shop 6h` opens the last six hours in `shop`; `:changes 1h`, also offered by the palette, the last hour.
- Clicking a name opens the object.
- Metrics charts mark recorded changes of a workload or node, and of a namespace those that start new pods. The rate strip above logs marks rollouts, scaling and config changes of the workload, or in the pod's namespace.
- AI agents can ask with the `what_changed` tool of the [MCP server](AGENTS.md).

## Where it is kept

In `history.db`, an SQLite database in Kubri's config folder next to `settings.json`, readable only by you. **What changed** under Settings turns recording off, and sets how many days it keeps (14 by default) and how large the database may grow (200 MB by default); the oldest changes go first. Deleting the file starts over.
