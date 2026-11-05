# Changelog

## 1.9.0 — 2026-11-05

- **Requests and limits in one dialog.** **Requests & limits…** (<kbd>Shift</kbd>+<kbd>Q</kbd>) on a workload, or on a pod for the workload that owns it, sets CPU and memory requests and limits for all its containers at once. It shows what the pods use and the room still free on the nodes and in the namespace's quotas, warns before a pod would no longer fit, and offers common values and steps with <kbd>↑</kbd> and <kbd>↓</kbd>. **Review…** in Right-sizing fills in all suggestions of a workload. See [Requests and limits](docs/CREATE.md#requests-and-limits).
- **Limits in the pod list.** CPU and Mem show the limit after what a pod uses, such as `120m / 500m`, with a bar that turns red at 90 %. The Nodes list shows **CPU free** and **Mem free**, the room left for new pods.
- **Start where you left off.** Kubri connects to the context you used last and opens the view you left. **On start** under Settings can follow the kubeconfig's current context instead. See [Clusters](docs/CLUSTERS.md#starting-where-you-left-off).
- **Port-forwards stay open.** Connections no longer end when the client is done sending, and idle connections no longer drop behind proxies. The Port-forwards page says how the last connection ended.
- **Favorite port-forwards with one click.** Favorites are listed under **Port-forwards** in the sidebar: one click starts a forward, another stops it. **Keep as a favorite** in the port-forward dialog keeps one as you start it. Starting on connect is now a choice per favorite; favorites kept before 1.9 still start on connect. See [Port-forwards that stay](docs/CREATE.md#port-forwards-that-stay).
- **Production namespaces.** In a context that is not production, namespaces can be marked as production, from the namespace menu or the context settings. They get the same care as a production context: a banner, typing the name before deleting, and no changes by AI agents. See [Settings](docs/SETTINGS.md#per-context).
- **Other label names for logs.** When the log collector names labels differently, such as `exported_namespace`, a label template on the Integrations page tells Kubri, for Logs in Grafana and the Loki log history. A template is kept once and picked per context. See [Other label names](docs/INTEGRATIONS.md#other-label-names).
- Pinned places come first in the sidebar. The Port-forwards page has bigger rows, and each favorite is a card with **Open** while it runs. The Integrations page gives the tools found, the tools missing and the settings each their own place, also in narrow windows.

## 1.8.0 — 2026-11-05

- **Kubri in German.** **Language** under Settings → Appearance picks English, German or the system's language, which is the default. All of Kubri is translated, including the findings of Checks and why a pod fits on no node. The words of Kubernetes stay English (Pod, Deployment, Namespace and the other kinds), and so do the columns of the lists and what the cluster itself says, such as events. See [Settings](docs/SETTINGS.md#appearance).
- **A Kubri for your team.** kubri-server can sign people in with your OpenID Connect provider, such as Keycloak, Dex, Entra ID, Google, Okta or GitLab. Each person gets a Kubri of their own that reaches the cluster with their own permissions, by impersonating them or with their own token, and every change goes to an audit log. **Activity** shows the team's changes. A Helm chart installs it in a cluster. See [A Kubri for your team](docs/TEAMS.md).
- **Feedback from the app.** `:feedback` and the links at the bottom of the help open a GitHub issue with your Kubri version and system filled in. New issue forms ask for what helps.
- **Coming from k9s:** a new page covers the first few minutes: what carries over, what works a little differently, and what there is besides. See [Coming from k9s](docs/FROM-K9S.md).
- Closing a shell in Kubri now ends the shell in the container too, instead of leaving `sh` running in the pod.

## 1.7.0 — 2026-11-05

- **Try Kubri without a cluster.** Without a kubeconfig, **Open the demo** opens a snapshot of a small shop that comes with the app, and **Start a cluster…** makes one on this computer with kind or k3d. **Try again** reads the kubeconfig files again. A live demo runs in the browser on the new website.
- **A new sidebar.** Overview, Clusters, Search and Create come first, then the main tools, **All tools** and **All kinds**, a page that finds any kind the cluster serves. Each view has one name everywhere: Overview, Topology, Access and Checks. The k9s commands such as `:pulses`, `:xray`, `:rbac` and `:popeye` still open them.
- **Why a container does not run.** For an image that cannot be pulled, a missing Secret or ConfigMap, out of memory, a crash loop, a pod with no room on any node and an autoscaler without metrics, Kubri says what is wrong in one sentence and offers the action that helps, such as **Set image…**, **Create Secret…** or **Raise memory limit…**. The logs, the pod inspector and the overview show it. See [Debugging](docs/DEBUGGING.md).
- **Switching contexts** shows a card with **Cancel** over the page, and says plainly when a cluster cannot be reached. The context switcher, the Clusters page and a strip on the overview show which contexts are reachable.
- **Lists fit the window.** When not every column fits, Kubri leaves out the least important ones first and says which. **All columns** (<kbd>w</kbd>, before **Wide**) shows them all and keeps the name in place while scrolling sideways. Empty lists offer **Create**, and an empty Helm page **Install chart**.
- **Actions in one place.** With the inspector open, an object's actions are there only. The row menu folds Copy, Go to and rarely used actions, and menus close when you go elsewhere. Typing a verb such as `restart` in the palette puts that action first. <kbd>Esc</kbd> twice leaves a terminal.
- **Roll back StatefulSets and DaemonSets** to an earlier revision from the inspector, like Deployments.
- **`kubri lint` and `kubri snapshot`** run the checks and save snapshots from the command line, for CI pipelines; `lint` also writes SARIF for GitHub code scanning. `kubri FILE.kubri` opens a snapshot in the app. See [Command line](docs/CLI.md).
- Read-only says the real reason, also when Kubri was started with `--read-only`. Scale puts the cursor in the number field, and an autoscaler's note is amber instead of red. CPU or memory requested above what the nodes hold is red. CronJob schedules read as words. Status colors in light mode have more contrast.
- The tour points at what it explains. Settings marks the section in view, the namespace menu can be searched, the palette says what to type, and Search shows that Enter runs it. Integrations lists the tools it did not find in a short list. Activity opens beside the page. The topology graph keeps names readable.
- **Web app:** the unused Prometheus query route is gone, so signed-in users can no longer run their own PromQL through the server. A broken pasted kubeconfig is reported as such.
- Drain checks that it may evict pods before it cordons the node. Opening a snapshot without a kubeconfig works, going back from a snapshot to the live cluster is quick, and the browser's Back works right after loading.

## 1.6.2 — 2026-11-04

- **Security, web app:** contexts added with **Add clusters…** can no longer run commands or read files on the server. Before, anyone who could sign in could add a context that signs in through a command, such as an exec plugin, and so run any command on the server. A context could also name a token file there and send its content to an address of their choice. The server now accepts such contexts only when it is started with `KUBRI_LOCAL_TERMINAL=true`. The desktop app is unchanged. See [Running Kubri in the browser](docs/WEB.md).
- **From URL** in Create no longer reads cloud metadata addresses such as `169.254.169.254`, also through redirects, and stops at 5 MB.
- In the web app, the address follows the view, so a reload keeps it and an address can be bookmarked or shared. The browser's Back button goes back through Kubri's views like <kbd>[</kbd>.
- Lists over several namespaces no longer bring back objects that were deleted in one of them.
- In the web app, a list that cannot start says why instead of "Lost connection to kubri-server".
- A port-forward for an object of another context is refused, like other actions on it, instead of reaching an object of the same name in the current context.
- The Clusters page keeps its connection to each cluster for 15 minutes, so a kubeconfig that signs in through a command such as `aws eks get-token` no longer runs it every minute.
- **Add clusters…** reads a pasted kubeconfig right away.
- In the desktop app, links to other sites, such as the install guides on the Integrations page, open in the browser.
- The Grafana, kubelet and coredns Services of kube-prometheus-stack are no longer taken for Prometheus.

## 1.6.1 — 2026-11-04

- Grafana needs only its address now. **Logs in Grafana** on a namespace, pod, workload or Service opens its logs in Grafana's Explore with the query already set. **Grafana…** lists that link first, then the matching dashboards, each with **Copy**. Kubri finds Grafana's Loki data source itself, or takes its name from the Integrations page. The editor for pasted dashboard addresses is gone. See [Integrations](docs/INTEGRATIONS.md#grafana).
- Loki history of a Service finds its lines with any log collector, by the names of the pods it selects.

## 1.6.0 — 2026-11-04

- Open Grafana from any object: paste a dashboard address once and Kubri fills in the namespace, pod or workload for each object. **Grafana…** in the object menus opens or copies it, also for a Grafana Kubri cannot read itself. See [Integrations](docs/INTEGRATIONS.md).
- The cluster overview shows what the cluster card shows: the health line, API server and version, a Ready workloads tile, and how much CPU and memory pods request.
- Terminals bring their own fonts, so prompts like powerlevel10k show their icons everywhere. **Settings > Terminal > Font** names an installed font to use first.
- In Docker, a mounted `~/.kube` is enough: Kubri finds every kubeconfig in it.
- While Kubri is read-only, change keys such as <kbd>e</kbd> or <kbd>Ctrl</kbd>+<kbd>D</kbd> say why they do nothing.
- When someone changed an object while you edit it, saving leads to **Compare** instead of showing the same message twice. Saving no longer leaves a "Keep your YAML changes?" question behind.
- Enter presses a button you reached with <kbd>Tab</kbd>. Deleting several objects puts the cursor in the field where you type `confirm`.
- The Scale field stays editable when you clear it, and an opened list shows the selected choice in the middle.
- The overview no longer counts crash-loop warnings as restarts, and shows small CPU use in millicores instead of "0.0 cores".

## 1.5.1 — 2026-11-02

- Logs of several containers or pods show in time order, also for the AI agent. Asking for the previous log of a container that never restarted says so.
- `:dp`, `:sec`, `:jo`, `:cr`, `:crb`, `:ro`, `:rb` and `:np` open their resources, as in k9s, also without your own `aliases.yaml`. A bare `:ctx` opens Clusters.
- A list filter written as `/regex/` works as a regular expression.
- The cursor stays on its row after Esc and while the list updates.
- The AI agent sees the real size of Secret values in bytes.
- A context whose cluster is missing in the kubeconfig names the missing cluster.
- Long kind names wrap in Resource health, and the drain dialog's grace field fits its text.

## 1.5.0 — 2026-11-02

- Switching back to a context you just left is instant: up to three recent contexts (Settings, 0 to 5) stay connected for 30 minutes, and Kubri returns to the view, filter, cursor and namespace you left there. `:ctx -` and the top entry of the context switcher go to the previous context; a green dot marks contexts that are still connected. See [Several clusters](docs/CLUSTERS.md#switching-back-is-instant).
- Logs and shells of other contexts keep running in the workspace, with the context named on their tab.
- A local terminal with `kubectl` and `helm` set to the context and namespace Kubri shows: <kbd>Ctrl</kbd>+<kbd>`</kbd>, `:term` or the terminal button. No credentials are written; read-only and locked contexts get none; production contexts get a warning line. In the web app it is off unless the server is started with `KUBRI_LOCAL_TERMINAL=true`. See [Local terminal](docs/TERMINAL.md).
- Load kubeconfig files from anywhere in the app; Kubri keeps reading them where they are. Without a usable kubeconfig, a start page explains what to do. Dialogs close on a click outside.
- The README's Docker command works as written, and the docs show how to use many kubeconfig files spread over `~/.kube`, also in Docker.

## 1.4.1 — 2026-10-31

- Refine the Link logo to match the selected design, including its slanted stem and curved cutout.
- Refresh native icons, favicon and branding assets.
- Show the application version in the sidebar and add reproducible README screenshot capture.
- Stabilize the benchmark cancellation test on Windows.

## 1.4.0

See the [v1.4.0 release](https://github.com/snaketech-tu/kubri/releases/tag/v1.4.0).
