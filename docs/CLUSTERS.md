# Several clusters

Kubri connects to one context at a time, the one in the top bar. The pages here look at several of them.

## Clusters

**Clusters** (`:fleet`, first in the sidebar) shows every context in your kubeconfig on one page, without connecting Kubri to any of them:

- whether its API server answers, its version and how long it took
- nodes, pods and workloads (Deployments, StatefulSets and DaemonSets), healthy out of all
- how much of the nodes' CPU and memory the pods request
- the name, color and label you gave the context in its settings, and whether it is production

Contexts are checked six at a time, each with its own ten-second limit, so a cluster that is down shows as **Unreachable** without holding up the others. Clusters that fail or cannot be reached are tinted red, those that need a look amber, and a line above the cards counts both. The page checks again every minute while it is open. Your user needs `list` on nodes, pods and the workload kinds across the cluster to see the counts; without it the card says the cluster is reachable but that nothing may be listed.

Clicking a cluster connects Kubri to it and opens its overview. When Kubri cannot connect at start, the welcome screen has **See all clusters** to find one that works.

## Adding clusters

**Add clusters…** on the Clusters page takes a kubeconfig dropped on it, chosen as a file or pasted, and lists its contexts. Pick the ones to add:

- Kubri keeps each one as its own file in `kubeconfigs` in Kubri's config folder, readable only by you. Your kubeconfig files are never changed.
- Its cluster and user are renamed after the context, so a cluster called `default` in the new file cannot clash with one of the same name in yours.
- Added contexts come after the files in `KUBECONFIG`, and Kubri hands the same list to helm and to k9s plugins, so they work there too.
- A context whose name is already taken, whose cluster or user is missing from the file, or that names a certificate or token file by a relative path cannot be added; the dialog says which. Contexts that sign in by running a command, like `aws` or `gke-gcloud-auth-plugin`, say so, since that command must be installed.

Added contexts have **Remove** on their card. Contexts from your own kubeconfig cannot be removed in Kubri.

## Limiting namespaces

In a context's settings (`:config`), **Only these namespaces** takes a list such as `shop, payments`, and **Or namespaces labeled** a label selector such as `team=shop`. The namespace picker then offers only those, and lists across all namespaces show only them, with one watch per namespace. That also suits users who may list only some namespaces. A namespace you pick or type yourself still opens.

## Search

**Search** (`:search`, also `:find`) finds objects by name, label and field, in the connected context or in several. Type part of a name and narrow it down:

| Part | Means |
| --- | --- |
| `web` | the name contains `web`; exact names come first, then names that start with it |
| `/^api-\d+$/` | the name matches a regular expression, ignoring case |
| `kind:deploy,svc` | these kinds, named as in the palette; `kind:all` every kind you may list |
| `ns:shop` or `-n shop` | one namespace |
| `-l app=web` or `label:app=web` | a label selector |
| `status.phase!=Running` | a field compared with `=`, `!=`, `~` (contains), `!~`, `>`, `<`, `>=` or `<=`; numbers compare as numbers |
| `ctx:*` or `ctx:prod,staging` | every context, or some; without it only the connected one |

Without `kind:` Kubri searches the kinds people look for by name: workloads, pods, Services, Ingresses, ConfigMaps, Secrets, volume claims, ServiceAccounts, nodes, namespaces and HTTPRoutes. A context limited to some namespaces is searched in those. Lists that fail, such as a context that is down, are named above the results and do not hold up the others. Up to 300 objects are shown.

Clicking a result opens it, connecting to its context first when it is another one. **Save search** puts the search in the sidebar under Search, kept in Kubri's settings file.

The palette (`:` or `Ctrl K`) also finds objects: it offers the objects opened lately, in any context, and while you type a word of three letters or more it looks for objects in the connected context whose name has it. **search …** at the end of the list opens the search page with what you typed.

## Compare

**Compare…** in an object's menu puts it beside the same object in another context or namespace, by default the next context with the same namespace and name. It shows the containers whose images differ, then the differences as a diff, for example a Deployment in staging and in production.

What each copy keeps of its own is left out: the namespace, `uid`, `resourceVersion`, `generation`, timestamps, managed fields, `last-applied-configuration`, the Deployment revision and the restart stamp of `kubectl rollout restart`. **Include status** compares the status too.

Kubri reads the other object with the other context's credentials. The connection is kept for five minutes, so comparing again is quick.
