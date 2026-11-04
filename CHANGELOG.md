# Changelog

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
