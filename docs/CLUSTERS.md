# Several clusters

Kubri connects to one context at a time, the one in the top bar. The pages here look at all of them.

## Clusters

**Clusters** (`:fleet`, first in the sidebar) shows every context in your kubeconfig on one page, without connecting Kubri to any of them:

- whether its API server answers, its version and how long it took
- nodes, pods and workloads (Deployments, StatefulSets and DaemonSets), healthy out of all
- how much of the nodes' CPU and memory the pods request
- the name, color and label you gave the context in its settings, and whether it is production

Contexts are checked six at a time, each with its own ten-second limit, so a cluster that is down shows as **Unreachable** without holding up the others. Clusters that fail or cannot be reached are tinted red, those that need a look amber, and a line above the cards counts both. The page checks again every minute while it is open. Your user needs `list` on nodes, pods and the workload kinds across the cluster to see the counts; without it the card says the cluster is reachable but that nothing may be listed.

Clicking a cluster connects Kubri to it and opens its overview. When Kubri cannot connect at start, the welcome screen has **See all clusters** to find one that works.

## Compare

**Compare…** in an object's menu puts it beside the same object in another context or namespace, by default the next context with the same namespace and name. It shows the containers whose images differ, then the differences as a diff, for example a Deployment in staging and in production.

What each copy keeps of its own is left out: the namespace, `uid`, `resourceVersion`, `generation`, timestamps, managed fields, `last-applied-configuration`, the Deployment revision and the restart stamp of `kubectl rollout restart`. **Include status** compares the status too.

Kubri reads the other object with the other context's credentials. The connection is kept for five minutes, so comparing again is quick.
