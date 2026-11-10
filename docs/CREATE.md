# Creating and changing

Everything `kubectl apply` and `helm upgrade` do, each with a preview of what will change first. This page also covers the parts of lists you can change: columns, copying and changing many objects at once, and port-forwards that stay.

## Create

Open **Create** in the sidebar, or type `:new` in the palette (`:create` and `:apply` work too). The editor takes YAML from any of these sources:

- **Add**: a template for a common kind (Deployment, Service, ConfigMap, Secret, Job, CronJob, Ingress, StatefulSet, DaemonSet, Pod, PersistentVolumeClaim, ServiceAccount, Namespace). For any other kind, including custom resources, Kubri builds a skeleton from the cluster's schema with the fields it requires. A Service or Ingress added after a workload already points at that workload.
- **Open files**, **From URL** and **Paste**. A GitHub page of a YAML file loads as the raw file. Kubri reads no address where clouds hand out the credentials of the machine it runs on, such as `169.254.169.254`.
- Short forms in the palette, like `kubectl create`:

  ```
  :new deploy web --image nginx:1.27 --replicas 3
  :new svc nodeport web --tcp 80:8080
  :new cm settings --from-literal mode=fast
  :new secret generic db --from-literal password=s3cret
  :new cronjob nightly --image busybox --schedule "0 3 * * *" -- sh -c date
  ```

Separate several objects with `---`. **Namespace** applies to objects that name no namespace.

**Review** (<kbd>Ctrl</kbd>+<kbd>S</kbd>) runs a server-side dry run of every object and shows what the API server would store: what it creates, how what it changes would differ, and what fails and why. **Apply** then runs it for real, like `kubectl apply --server-side`, with Kubri as the field manager. Objects go in dependency order: namespaces, CRDs, config and RBAC before the workloads that use them. A CRD and its custom resources can go in one batch, because Kubri waits for the new kind to be served before the resources that use it.

### Guided forms

When the editor holds one Deployment, Service, Job, CronJob, ConfigMap or Secret, a form beside it edits the common fields: name, image, command, ports, replicas, requests and limits, schedule, selector, data. The form writes the same YAML the editor shows, and your edits in the YAML show in the form. Renaming a workload also renames its `app` label and container, so the selector keeps matching.

### Clone

**Clone to…** in an object's menu opens Create with a copy of it. Kubri strips the fields the cluster assigned (status, uid, resource version, managed fields, owner references, cluster IPs, node ports and similar). Choose the **Context** and **Namespace** to create it in, rename it if you like, and review. A production context shows a note, and a read-only one refuses.

## Helm

The Helm view (`:helm`) lists releases from the records Helm keeps in the cluster. Changes go through the `helm` command, which the desktop app finds on `PATH` and the Docker image includes.

| What | How |
| --- | --- |
| Install | **Install chart** (<kbd>i</kbd>). Search Artifact Hub, or give a chart by name with its repository URL, as `repo/chart` for a repository helm knows, or as `oci://registry/path/chart`. **Default values** loads the chart's values into the editor. Review renders the chart against the cluster (`--dry-run=server`) and lists every object it would create. |
| Upgrade | **Upgrade** (<kbd>u</kbd>) on a release. Edit its values and keep the chart it runs, or pick another chart or version. The review shows how the values and each rendered object would change. When the chart stays the same, Kubri takes it from the release record and fetches its subcharts again; if that fails, as at a registry's rate limit, give the chart under **Another chart or version**. |
| Compare | The **Compare** tab (<kbd>c</kbd>) shows the values and each object that differ between two revisions. The rollback dialog shows the same comparison before you confirm. |
| Drift | The **Drift** tab (<kbd>x</kbd>) dry-runs the deployed manifest against the cluster, like `kubectl diff --server-side`, and shows which objects were changed or deleted by hand. An upgrade or rollback puts them back. Fields that someone added to an object without touching the release's own fields do not show. |
| Roll back, uninstall | <kbd>r</kbd> and <kbd>Ctrl</kbd>+<kbd>D</kbd>, as before. |

## Lists

### Columns

**Columns** above a list chooses its columns for that kind: hide or show any, including those of the wide view, and move them up or down. **Add a column** takes a name and a JSONPath, like `kubectl get -o custom-columns`:

| Path | Shows |
| --- | --- |
| `.spec.nodeName` | a field |
| `.spec.template.spec.containers[*].image` | every container's image, separated by commas |
| `.status.conditions[0].type` | one element of a list (`[-1]` for the last) |
| `.metadata.labels['app.kubernetes.io/name']` | a key with dots, also written `.metadata.labels.app\.kubernetes\.io/name` |
| `.status.conditions[?(@.type=="Ready")].status` | the elements of a list whose field is (`==`) or is not (`!=`) a value |

Other filters and recursive descent (`..`) are not supported. The choices are kept per kind in Kubri's settings file, so they survive a restart and apply in the desktop app and the browser alike. **Reset** goes back to the kind's [view](VIEWS.md) if it has one, otherwise to the server's columns.

When a list has more columns than fit the window, Kubri leaves out the least important ones first, such as percentages, ages and restarts, and always keeps the name and status. Long names are cut to leave room for the rest. The footer says which columns it left out. **All columns** (<kbd>w</kbd>) shows every column and lets the list scroll sideways, with the name staying in place.

An empty list of a kind you may create offers **Create** right there, and an empty Helm page offers **Install chart**. Kinds without a state, such as Secrets and ConfigMaps, have no **Needs attention** tab.

### Copy and change many

- Right-click a cell to copy its value, or the whole column as the list shows it.
- Mark rows with <kbd>Space</kbd> (or <kbd>Ctrl</kbd>+<kbd>Space</kbd> for a range) to copy their names, **Scale** (<kbd>s</kbd>) them all to the same count, **Restart** (<kbd>r</kbd>) them all, or delete them.

## Requests and limits

**Requests & limits…** (<kbd>Shift</kbd>+<kbd>Q</kbd>, in the menus of Deployments, StatefulSets, DaemonSets and their pods) shows the CPU and memory requests and limits of every container of the workload in one dialog. On a pod it opens the workload that owns it.

- <kbd>↑</kbd> and <kbd>↓</kbd> in a field step through common sizes, such as 100m, 150m, 200m or 128Mi, 192Mi, 256Mi. You can also type any amount Kubernetes takes.
- An emptied field removes the value. A container without a request gets its limit as the request, as in Kubernetes.
- Below the fields, the dialog shows the room on the nodes the pods may run on: what other pods request there, what these pods request now and with the new values, and how many pods of the new size fit. It warns when a pod fits on no node, when fewer pods fit than the workload runs, when the nodes would be more than 90% requested, when a new pod only fits once an old one is gone, and when a quota of the namespace would be exceeded. The warnings do not stop you, for example when you add nodes next.
- Setting the values patches all containers at once, after the diff of the dry run, and replaces the pods with a rolling update.

## Port-forwards that stay

- **Start a port-forward…** on the port-forwards page (<kbd>Shift</kbd>+<kbd>F</kbd> there) asks for the namespace, then a deployment, StatefulSet, DaemonSet, service or pod in it, and the port. The list of a kind does the same with <kbd>Shift</kbd>+<kbd>F</kbd> on the selected row.
- A forward to a Service or workload moves to another ready pod when its pod goes away, as after a restart or rollout. A forward straight to a pod keeps using that pod's name.
- **Favorite** (<kbd>s</kbd> on the port-forwards page, or **Keep as a favorite** when you start one) keeps a forward in its context's settings, on the same local port.
- The favorites of the connected context are listed under **Port-forwards** in the sidebar. Click one to start it, and click it again to stop it. A filled dot means it is running.
- The port-forwards page lists the favorites too, to start, stop or remove them. With **Start on connect**, Kubri starts a favorite each time it connects to its context. Favorites kept before Kubri 1.9 start on connect; new ones do not until you turn it on.
- <kbd>o</kbd> opens a forward in the browser, and <kbd>c</kbd> copies its URL.
- Quiet connections stay open: Kubri pings the cluster every 20 seconds, so load balancers and proxies in front of the API server do not drop them as idle.
- A client that stops sending after its request, as some HTTP and database clients do, still gets the whole answer. The cluster's port-forward cannot tell the pod that the client is done, so Kubri keeps the connection until the pod closes it, or until the pod has sent nothing for 15 seconds.
- A client that connects and leaves without sending anything, such as a check whether the port is open, frees the pod's connection after a second. When the pod resets a connection whose client has already left, as MariaDB does with one that never signed in, that is not shown as an error.
- With the address `localhost`, a forward listens on both 127.0.0.1 and ::1, like kubectl, so clients reach it whichever one they try.
- Each row says how its last connection ended: closed by the pod, closed by the client, or the error, in red.
