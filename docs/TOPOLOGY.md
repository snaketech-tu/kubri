# What depends on what

## Graph

**Graph** on the Topology page (`:graph`, or `:graph shop` for one namespace) draws the objects of the namespace, or of all of them, as one graph, left to right:

- Ingresses, then the Services they route to
- the workloads behind each Service (Deployments, StatefulSets, DaemonSets, CronJobs and Jobs), with the ConfigMaps, Secrets and claims they use beneath them
- their pods, those with problems first, up to eight per workload; the workload says how many more there are
- with **Group by: Node**, the node each pod runs on

Each object has a stripe in its health color, and a red box with the reason when it is failing. Something an object names that is not there, like a Secret a Deployment reads or the Service an Ingress routes to, is drawn with a dashed red outline and **Missing**.

**Group by** puts the objects in bands by namespace, by application (the `app.kubernetes.io/name`, `app` or `k8s-app` label, or that of what an object connects to) or by node. Pointing at an object lights up what it connects to. **Find** and **Problems** dim the rest. Scroll or pinch to zoom, drag to move, and **Fit** shows it all. Clicking an object opens it. **Trees** goes back to the XRay trees. The graph refreshes every 15 seconds.

## Request paths

Below the graph, each host and path of each Ingress is followed the way a request goes: Ingress → Service → EndpointSlice → pods. The first link that breaks is red and says why:

- the Service does not exist
- the Service has no port the Ingress names
- the Service selects no pod, with the labels it looks for
- none of the pods behind it is ready

Pointing at a path lights it up in the graph.

## Used by and deleting

**Used by** (`u`, in the menu of ConfigMaps, Secrets, PersistentVolumeClaims, ServiceAccounts, PriorityClasses and Services) lists what uses an object in its namespace:

- workloads, CronJobs, Jobs and bare pods whose pod spec names a ConfigMap, Secret, claim, ServiceAccount or PriorityClass, in volumes, projected volumes, environment variables, `envFrom` or `imagePullSecrets`
- Ingresses that serve TLS from a Secret
- Ingresses and Gateway API HTTPRoutes that send traffic to a Service

The delete dialog looks this up before you confirm. When something uses the object, it says so in red, for example "It is used by 3 Deployments and 1 Ingress for TLS", and names them, so you see what breaks or stops starting before you delete it. With several objects marked, each one is checked. When Kubri cannot look, for lack of permission or because the API does not answer, the dialog says nothing and the delete still works.
