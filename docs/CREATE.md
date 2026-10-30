# Creating and changing

Everything `kubectl apply` and `helm upgrade` do, each with a preview of what will change first. This page also covers the parts of lists you can change: columns, copying and changing many objects at once, and port-forwards that stay.

## Create

Open **Create** in the sidebar, or type `:new` in the palette (`:create` and `:apply` work too). The editor takes YAML from any of these sources:

- **Add**: a template for a common kind (Deployment, Service, ConfigMap, Secret, Job, CronJob, Ingress, StatefulSet, DaemonSet, Pod, PersistentVolumeClaim, ServiceAccount, Namespace). For any other kind, including custom resources, Kubri builds a skeleton from the cluster's schema with the fields it requires. A Service or Ingress added after a workload already points at that workload.
- **Open files**, **From URL** and **Paste**. A GitHub page of a YAML file loads as the raw file.
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

### Copy and change many

- Right-click a cell to copy its value, or the whole column as the list shows it.
- Mark rows with <kbd>Space</kbd> (or <kbd>Ctrl</kbd>+<kbd>Space</kbd> for a range) to copy their names, **Scale** (<kbd>s</kbd>) them all to the same count, **Restart** (<kbd>r</kbd>) them all, or delete them.

## Port-forwards that stay

- A forward to a Service or workload moves to another ready pod when its pod goes away, as after a restart or rollout. A forward straight to a pod keeps using that pod's name.
- **Favorite** (<kbd>s</kbd> on the port-forwards page) keeps a running forward in its context's settings, on the same local port. Each time Kubri connects to that context, it starts its favorites that are not running yet. The page also lists the favorites that are not running, to start or remove.
- <kbd>o</kbd> opens a forward in the browser, and <kbd>c</kbd> copies its URL.
