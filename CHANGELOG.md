# Changelog

## 1.12.0 — 2026-11-11

- **Arrow keys like tabs.** <kbd>Tab</kbd> moves between the parts of the window: the sidebar, a row of tabs, a toolbar and the list are one stop each, and inside them the arrow keys move, with <kbd>Home</kbd> and <kbd>End</kbd> for the first and last. On the Overview the arrow keys move between the tiles, and <kbd>Shift</kbd>+<kbd>F10</kbd> or the menu key opens the menu of the row under the cursor. See [From k9s](docs/FROM-K9S.md).
- **A list opens with its first row chosen.** Opening Pods or any other list shows the cursor on the first row at once, with its actions above the list, and the list takes the keys, so <kbd>Enter</kbd> opens that row.
- **The inspector without the mouse.** <kbd>Tab</kbd> goes into the inspector, <kbd>←</kbd> <kbd>→</kbd> change its tab, <kbd>↑</kbd> <kbd>↓</kbd> and the page keys scroll, and <kbd>Esc</kbd> goes back to the list.
- **Back with Backspace, and breadcrumbs you can click.** <kbd>Backspace</kbd> goes back, and first closes an open inspector. The breadcrumb above each page leads back too: the context to its Overview, the namespace to the namespace list, and after a drill-down to the list you came from, with the object under the cursor.
- **The command palette completes.** `:po sh` opens the pods of `shop`. <kbd>Enter</kbd> runs the first suggestion, shown in grey after what you typed, and <kbd>→</kbd> or <kbd>Tab</kbd> takes it into the line. A namespace that does not exist is named in red instead of opening an empty list.
- **A YAML editor with the usual keys.** Find and replace, go to a line, <kbd>Ctrl</kbd>+<kbd>D</kbd> deletes the line, <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>D</kbd> duplicates it, and <kbd>Alt</kbd>+<kbd>J</kbd> and <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>L</kbd> give several cursors. Brackets and quotes close as you type. See [Settings](docs/SETTINGS.md).
- **Gentler on your clusters.** Pages that cannot watch ask less often (the inspector every 15 seconds, CPU and memory in lists every 30), nothing is asked while the window is minimized or the tab is in the background, and after a change made in Kubri open pages ask again right away. The Clusters page asked every cluster again and again while it was open; it now checks every two minutes. See [Clusters](docs/CLUSTERS.md#how-often-kubri-asks-the-cluster).
- **Start a port-forward from the Port-forwards page.** **Start a port-forward…** (<kbd>Shift</kbd>+<kbd>F</kbd> there) asks for the namespace, then a deployment, StatefulSet, DaemonSet, service or pod, and the ports. The local port comes first in both port-forward dialogs, and a port typed while the ports load stays.

## 1.11.1 — 2026-11-10

- Tidier code: duplicated helpers and unused code are gone. Nothing changes in how Kubri works.

## 1.11.0 — 2026-11-10

- **Kubri for Windows on Arm.** A native installer for Windows arm64 comes with each release, next to the one for x64.
- **The Helm chart as a package.** Each release publishes the chart to `oci://ghcr.io/snaketech-tu/charts/kubri`, so installing kubri-server needs no clone of the repository. See [Install it with Helm](docs/TEAMS.md#install-it-with-helm).
- **Settings that survive upgrades.** With `persistence.enabled=true`, the chart keeps Kubri's settings on a volume, so context colors and locks, integrations, saved searches and views stay when the pod restarts or the chart is upgraded.
- **Builds you can check.** The installers and the Docker image come with GitHub attestations, and the image with a list of what it contains (SBOM). `gh attestation verify` shows that a file was built from this repository. See [Running it safely](docs/TEAMS.md#running-it-safely).
- **Running a team Kubri safely.** A new section of the team guide covers the recommended setup, the audit log, the local terminal and upgrades.
- Under the hood, the desktop app and the web server now make every call to the core in the same way, and the types the UI uses are written from the core's own. The web server's internal API changed; nothing outside Kubri used it.

## 1.10.1 — 2026-11-08

- **Shells and terminals close cleanly.** Typing into a shell that has just ended or been closed no longer sends the input anywhere, and a shell that ends shows its message once, in the desktop app and in the browser.
- **AI agent requests that were given up go away.** When an AI agent stops waiting for your approval, its request leaves the approval window and can no longer be allowed by mistake.
- **Newer libraries.** Kubri now talks to clusters with kube 5 and the Kubernetes 1.37 types, and all its libraries are up to date. A proxy set in the kubeconfig (`proxy-url`) and `NO_PROXY` keep working as before.

## 1.10.0 — 2026-11-07

- **An overview that answers "is there room?"** The CPU and memory cards show how much of the cluster running pods reserve with their requests, which decides whether a new pod fits, and below it what is in use right now. A pod that cannot start names itself in one sentence, such as "ledger cannot start: it asks for 64 cores", instead of pushing the card past 100 %. Each problem is counted once: Resource health no longer repeats pods, nodes and ReplicaSets. See [Clusters](docs/CLUSTERS.md).
- **Warnings by workload.** **Needs attention** shows one entry per Deployment, StatefulSet, DaemonSet or CronJob, with how many pods are affected, in plain words, and for a container that keeps restarting, the last line it logged before it stopped. The raw warnings fold away under **Recent warnings**. See [Debugging](docs/DEBUGGING.md).
- **Amounts you can read.** CPU reads in cores (0.25, 1.5 cores) and memory in MiB and GiB. **Settings → Appearance → CPU and memory** switches back to kubectl's units (250m, 512Mi); the CSV export and k9s plugins always get those. See [Settings](docs/SETTINGS.md#appearance).
- **Throttled pods stand out.** A pod at its CPU limit says **throttled**, one close to its memory limit **near memory limit**, and both count under Needs attention. The inspector of a pod starts with **Use right now**: CPU and memory against the limit, without Prometheus. The Nodes list calls the room for new pods **CPU left** and **Mem left**.
- **Namespaces at a glance.** The Namespaces list shows how many pods are healthy, and what they use and request. A namespace with failing pods needs attention.
- **Production namespaces where you see them.** A shield in the Namespaces list and in the namespace picker marks a namespace as production with one click, and rules in the context settings mark them by name (`*-prod`) or label (`env=production`). Kubri suggests namespaces that look like production. Production shows as a red stripe and a **Production** badge, and AI agents can change nothing in it, the namespace itself included. See [Settings](docs/SETTINGS.md#production-namespaces).
- One health color everywhere: a Deployment with no pod ready is red, a pod's Ready takes the color of its status, and a finished pod no longer counts as not ready. "Controlled by" names a pod's Deployment, conditions fold when all are true, restarts read as `5 · 1m ago`, and Right-sizing shows a container's CPU and memory as one block.

## 1.9.2 — 2026-11-06

- **Resizable sidebar.** Drag the sidebar's edge to make it wider, or focus it and use the arrow keys. Kubri remembers the width; a double-click or <kbd>Home</kbd> goes back to the default, which is also the smallest width.
- **No more jumping layout.** The action bar above a list keeps to one line, with the buttons that do not fit under **More**, and always keeps its place, so clicking a row, loading a list or filtering it empty no longer moves the rows. Columns are drawn once when a list opens, not again when its metrics come. The inspector, What changed and Helm no longer jump while they load.
- **Clearer checkboxes.** Checkboxes are bigger, with a clear edge in light and dark. The marks in lists stay in sight.
- **Port-forwards:** a client leaving at the moment the pod resets its connection no longer shows up as a failed forward.

## 1.9.1 — 2026-11-06

- **Port-forwards work on the first try.** A client that only checks whether the port is open no longer keeps the pod's connection, so databases such as MariaDB no longer reset it and show an error. With the address `localhost`, a forward listens on both 127.0.0.1 and ::1, like kubectl, so clients reach it whichever one they try.
- **Node use against what a node can give.** CPU and Mem in the Nodes list show what the node uses next to what it can give to pods, such as `292m / 4000m`, with a bar that turns red at 90 %.
- **Wider inspector.** The details panel opens wider, and container images stay readable: long images break at `/`, `:` and `@`, and digests are shortened, with the full image in the tooltip.

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
