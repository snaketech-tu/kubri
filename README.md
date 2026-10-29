<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/assets/kubri-logo-dark.svg">
    <img src="docs/assets/kubri-logo.svg" alt="Kubri" width="300">
  </picture>
</p>

<p align="center">
  <b>A fast Kubernetes GUI that does everything k9s does, by keyboard or by mouse.</b><br>
  A desktop app for Windows, macOS and Linux, or a web app for your whole team.
</p>

<p align="center">
  <a href="https://github.com/snaketech-tu/kubri/actions/workflows/ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/snaketech-tu/kubri/ci.yml?branch=main&label=CI" alt="CI"></a>
  <a href="LICENSE"><img src="https://img.shields.io/github/license/snaketech-tu/kubri?color=0969da" alt="License: Apache-2.0"></a>
  <img src="https://img.shields.io/badge/runs%20on-Windows%20%C2%B7%20macOS%20%C2%B7%20Linux%20%C2%B7%20browser-0969da" alt="Runs on Windows, macOS, Linux and in the browser">
</p>

<p align="center">
  <a href="#install">Install</a> ·
  <a href="#features">Features</a> ·
  <a href="#screenshots">Screenshots</a> ·
  <a href="#coming-from-k9s">Coming from k9s</a> ·
  <a href="#documentation">Docs</a>
</p>

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/assets/screenshots/pods-dark.png">
  <img src="docs/assets/screenshots/pods-light.png" alt="Kubri listing the pods of a namespace, with CPU and memory against their requests">
</picture>

## Why Kubri

<table>
  <tr>
    <td width="33%" valign="top"><b>Everything k9s does</b><br>Every resource and CRD, live. Drill-downs, logs, shells, port-forwards, Pulses, XRay, RBAC, Helm, a sanitizer and benchmarks. Your k9s aliases, hotkeys, plugins and skins work as they are.</td>
    <td width="33%" valign="top"><b>Keyboard or mouse</b><br>The keys are the ones you know from k9s, <kbd>:</kbd> opens the command palette and <kbd>?</kbd> lists them all. Or click: every action is a button, and a right click on a row lists everything you can do with it, with its key.</td>
    <td width="33%" valign="top"><b>Desktop or browser</b><br>A small native app built with Rust and Tauri, one container for your team with a password sign-in, or a pod in the cluster with its service account.</td>
  </tr>
</table>

## Install

### Desktop app

Download it for Windows, macOS or Linux from the [releases](https://github.com/snaketech-tu/kubri/releases). It uses the kubeconfig on your computer, just like kubectl.

### In the browser

```sh
docker run --rm -p 127.0.0.1:7878:7878 \
  --user "$(id -u)" \
  -v "$HOME/.kube/config:/kubeconfig:ro" -e KUBECONFIG=/kubeconfig \
  -e KUBRI_PASSWORD='a long password' \
  ghcr.io/snaketech-tu/kubri
```

Then open <http://127.0.0.1:7878>. To run it in a cluster for your team, with [`deploy/kubernetes.yaml`](deploy/kubernetes.yaml), see [Running Kubri in the browser](docs/WEB.md).

### From source

See [Development](#development).

## Features

<table>
  <tr>
    <td width="50%" valign="top">
      <b>Browse</b>
      <ul>
        <li>Every resource type and CRD, live, with the columns of <code>kubectl get</code> (<kbd>w</kbd> for the wide view)</li>
        <li>k9s filters: regular expressions, <code>!</code> to invert, <code>-f</code> fuzzy, <code>-l app=web</code></li>
        <li><kbd>Enter</kbd> drills down, <kbd>Shift</kbd>+<kbd>J</kbd> jumps to the owner, <kbd>[</kbd> <kbd>]</kbd> go back and forth</li>
        <li>CPU and memory against requests, limits and node capacity</li>
        <li>Choose, order and add columns per kind, JSONPath columns included; copy a cell or a whole column</li>
      </ul>
    </td>
    <td width="50%" valign="top">
      <b>Act</b>
      <ul>
        <li>Create from templates, files, URLs or the clipboard (<kbd>:new</kbd>), with forms for common kinds and a dry run first</li>
        <li>Clone an object to another namespace or context</li>
        <li>Delete, scale, restart, set images and roll back, one object or many</li>
        <li>Edit YAML, labels and annotations</li>
        <li>Run and suspend CronJobs</li>
        <li>Cordon, uncordon and drain nodes with the checks of <code>kubectl drain</code></li>
      </ul>
    </td>
  </tr>
  <tr>
    <td valign="top">
      <b>Debug</b>
      <ul>
        <li>Live logs of a pod or a whole workload, all containers merged, with new pods joining during a rollout</li>
        <li>Levels, JSON and logfmt fields as a table, field filters like <code>status>=500</code>, folded stack traces and a rate strip</li>
        <li>Shell or attach in a real terminal</li>
        <li>A workspace of log and shell tabs that keep running while you browse, docked below or beside the list, two side by side</li>
        <li>Debug containers, debug copies of crash-looping pods and node shells</li>
        <li>A fix with each likely problem: a higher memory limit with its diff, why each node refuses a pending pod, why a Service has no endpoints</li>
        <li>Port-forwards that stay: favorites start with Kubri and move to a new pod after a restart, with HTTP benchmarks on top</li>
        <li>Decoded Secrets, and what uses a ConfigMap or Secret</li>
      </ul>
    </td>
    <td valign="top">
      <b>See the whole cluster</b>
      <ul>
        <li>Pulses: health per kind, usage trends and recent warnings</li>
        <li>XRay: each workload down to its pods, containers and references, or a namespace as a graph with each Ingress's request path and where it breaks</li>
        <li>RBAC: who may do what, as a matrix of verbs</li>
        <li>Helm: install charts from repositories, OCI or Artifact Hub, upgrade with a diff, compare revisions and spot drift</li>
        <li>A Popeye-style sanitizer with a grade</li>
        <li>Prometheus charts for CPU, memory, restarts, network and requests, with Grafana dashboards and Loki history</li>
        <li>Flux, Argo CD and cert-manager: status, reconcile, sync and renew, and a warning before changing what they manage</li>
        <li>Right-sizing: containers that request far more or less than they use, with the change to fix it</li>
        <li>Every cluster on one page, with contexts added from a kubeconfig in the app, and an object compared across clusters</li>
        <li>Search by name, label and field across clusters (<kbd>:search</kbd>), with saved searches in the sidebar</li>
      </ul>
    </td>
  </tr>
  <tr>
    <td valign="top">
      <b>Work safely</b>
      <ul>
        <li>A name, color and label per context; production shows a banner and asks for the name before deleting</li>
        <li>Read-only and locked contexts, unlocked for 15 minutes when needed</li>
        <li>Actions you may not run say which permission is missing, with a Role to send to an admin</li>
        <li>Every action as the <code>kubectl</code> command that does the same</li>
        <li>Deleting something still in use names what breaks first</li>
      </ul>
    </td>
    <td valign="top">
      <b>Edit with confidence</b>
      <ul>
        <li>Changes run as a server-side dry run first and show what would change</li>
        <li>Field docs, completion and checks in the YAML editor, from the cluster's own schema</li>
        <li>Changes others make while you edit are merged with yours, not lost</li>
      </ul>
    </td>
  </tr>
  <tr>
    <td valign="top">
      <b>Your k9s setup</b>
      <ul>
        <li>Aliases, hotkeys, plugins, skins and jumps work as they are</li>
        <li><code>readOnly</code>, <code>defaultView</code>, thresholds and log settings, per context</li>
      </ul>
    </td>
    <td valign="top">
      <b>Run it anywhere</b>
      <ul>
        <li>Desktop app for Windows, macOS and Linux</li>
        <li>In the browser with a password sign-in, from Docker or in the cluster</li>
        <li>A read-only mode that never changes the cluster</li>
        <li>An MCP server, so AI agents can read your clusters with your permissions</li>
        <li>Explain: ask your own model why a pod fails, seeing first what is sent</li>
      </ul>
    </td>
  </tr>
</table>

## Screenshots

<table>
  <tr>
    <td width="50%" valign="top"><img src="docs/assets/screenshots/overview.png" alt="Cluster overview: pods and nodes, CPU and memory, and the objects that need attention"><br><b>Overview</b>: what needs attention first, with health and trends (<kbd>:pulses</kbd>)</td>
    <td width="50%" valign="top"><img src="docs/assets/screenshots/xray.png" alt="Topology: deployments down to their replica sets and pods"><br><b>Topology</b>: from a Deployment down to its pods (<kbd>:xray</kbd>)</td>
  </tr>
  <tr>
    <td valign="top"><img src="docs/assets/screenshots/describe.png" alt="A crash-looping pod in the inspector beside the list"><br><b>Inspector</b>: status, the likely problem, events and previous logs beside the list</td>
    <td valign="top"><img src="docs/assets/screenshots/palette.png" alt="The command palette suggesting resources"><br><b>Command palette</b>: <kbd>:</kbd> and any resource or command</td>
  </tr>
  <tr>
    <td valign="top"><img src="docs/assets/screenshots/helm.png" alt="A Helm release with its objects, notes and revisions"><br><b>Helm</b>: releases, values, manifests, history and rollback</td>
    <td valign="top"><img src="docs/assets/screenshots/rbac.png" alt="Access: a service account's role bindings and the rules they grant"><br><b>Access</b>: who may do what, from the role bindings (<kbd>:rbac</kbd>)</td>
  </tr>
  <tr>
    <td valign="top"><img src="docs/assets/screenshots/safety.png" alt="Setting an image on a production context: the banner, and the change as the API server would store it"><br><b>Production guard rails</b>: a colored context, and every change as a dry-run diff first</td>
    <td valign="top"><img src="docs/assets/screenshots/editor.png" alt="Editing a Deployment's YAML with completion and field docs from the cluster's schema"><br><b>YAML editor</b>: completion and field docs, like <code>kubectl explain</code></td>
  </tr>
  <tr>
    <td valign="top"><img src="docs/assets/screenshots/explain.png" alt="Explain beside a crash-looping pod: the likely cause and how to fix it"><br><b>Explain</b>: ask your own model why a pod fails, after seeing what is sent</td>
    <td valign="top"><img src="docs/assets/screenshots/agents.png" alt="Settings for AI agents: the MCP endpoint, the command for Claude Code and the log of tool calls"><br><b>AI agents</b>: Kubri as an MCP server, with every call logged (<kbd>:agents</kbd>)</td>
  </tr>
  <tr>
    <td valign="top"><img src="docs/assets/screenshots/logs.png" alt="A Deployment's merged logs as a table of JSON fields, filtered by status>=500"><br><b>Structured logs</b>: fields as columns, filters like <code>status>=500</code>, and a rate strip</td>
    <td valign="top"><img src="docs/assets/screenshots/workspace.png" alt="The pod list with a shell and a Deployment's logs side by side in the workspace below"><br><b>Workspace</b>: logs and shells that keep running while you browse</td>
  </tr>
  <tr>
    <td valign="top"><img src="docs/assets/screenshots/create.png" alt="Create with a Deployment's YAML and a form for its name, replicas, image, port and resources"><br><b>Create</b>: templates, short forms like <code>:new deploy web --image nginx</code> and forms that write the YAML</td>
    <td valign="top"><img src="docs/assets/screenshots/helm-upgrade.png" alt="A Helm upgrade review: the changed values and the Deployment whose replicas and image change"><br><b>Helm upgrade</b>: change values and see every object that changes before you upgrade</td>
  </tr>
  <tr>
    <td valign="top"><img src="docs/assets/screenshots/metrics.png" alt="A Deployment's CPU and memory per pod from Prometheus, against its request and limit, with a link to its Grafana dashboard"><br><b>Metrics</b>: Prometheus history against requests and limits, and the matching Grafana dashboard</td>
    <td valign="top"><img src="docs/assets/screenshots/right-sizing.png" alt="Right-sizing: containers whose requests are far from their use, with suggested values"><br><b>Right-sizing</b>: what each container requests and uses, and what to set instead (<kbd>:rightsizing</kbd>)</td>
  </tr>
  <tr>
    <td valign="top"><img src="docs/assets/screenshots/fleet.png" alt="Clusters: a card per context with its health, nodes, pods, workloads and requests, including unreachable ones"><br><b>Clusters</b>: every context side by side, even when some are down (<kbd>:fleet</kbd>)</td>
    <td valign="top"><img src="docs/assets/screenshots/search.png" alt="Search for web across two contexts, listing Deployments, Services and Ingresses"><br><b>Search</b>: objects by name, label and field in several clusters at once (<kbd>:search</kbd>)</td>
  </tr>
  <tr>
    <td valign="top"><img src="docs/assets/screenshots/graph.png" alt="Topology graph of a namespace: Ingresses, Services, a Deployment with its Secret and pod, and request paths with two broken links"><br><b>Topology graph</b>: how a namespace connects, and where a request path breaks (<kbd>:graph</kbd>)</td>
    <td valign="top"><img src="docs/assets/screenshots/delete.png" alt="Deleting a Secret: the dialog warns it is used by a Deployment and by an Ingress for TLS"><br><b>Blast radius</b>: what breaks if you delete it, before you confirm</td>
  </tr>
</table>

## Coming from k9s

Kubri reads your k9s configuration and uses the same keys, so you can start right away. The ones you will use most:

| Key | What it does |
| --- | --- |
| <kbd>:</kbd> | Go to a resource or run a command, like `:deploy kube-system`, `:po shop app=web` or `:ctx prod` |
| <kbd>/</kbd> | Filter the list |
| <kbd>Enter</kbd> | Drill down, from a Deployment to its pods |
| <kbd>d</kbd> <kbd>y</kbd> <kbd>e</kbd> | Describe, YAML, edit |
| <kbd>l</kbd> <kbd>s</kbd> | Logs, shell |
| <kbd>Shift</kbd>+<kbd>F</kbd> | Port-forward |
| <kbd>Ctrl</kbd>+<kbd>D</kbd> | Delete |
| <kbd>?</kbd> | All keys |

[Using your k9s configuration](docs/K9S.md) lists what Kubri covers and what it does not.

## How it works

Kubri uses the kubeconfig you already have: every file listed in `KUBECONFIG`, or `~/.kube/config` if that is not set. It reads them as kubectl does: listed files that do not exist are skipped, and a context works in Kubri when it works with kubectl, with kubectl's error message when it does not. It connects to your `current-context` on startup, and you can switch contexts at any time from the top bar (or with <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>C</kbd>). Started from the Dock, Finder or a desktop launcher, the desktop app takes the environment of your login shell, such as `PATH` and `KUBECONFIG`, so `helm`, credential plugins such as `aws` or `gke-gcloud-auth-plugin`, and the commands of k9s plugins are found as in a terminal.

| Part | Path | What it is |
| --- | --- | --- |
| Core | [`crates/kubri-core`](crates/kubri-core) | All Kubernetes access, built on [kube-rs](https://kube.rs). Shared by both apps. |
| Web server | [`crates/kubri-server`](crates/kubri-server) | Serves the UI and a REST + WebSocket API on `127.0.0.1:7878`. |
| Desktop app | [`ui/src-tauri`](ui/src-tauri) | [Tauri 2](https://tauri.app) shell that calls the core directly. |
| UI | [`ui`](ui) | SvelteKit + TypeScript single-page app, used by both. |

## Documentation

- [Running Kubri in the browser](docs/WEB.md): settings, sign-in, the Docker image, running it in a cluster and behind a proxy.
- [AI agents (MCP)](docs/AGENTS.md): connecting Claude Code, Claude Desktop or another agent, the tools it gets, and what it may not do.
- [Explain](docs/EXPLAIN.md): asking a model about a pod, logs, a finding or a failed action with your own key, and what is hidden.
- [Creating and changing](docs/CREATE.md): Create, forms and cloning, Helm install, upgrade, compare and drift, columns, bulk changes and port-forwards that stay.
- [Several clusters](docs/CLUSTERS.md): the Clusters page, adding contexts from a kubeconfig, namespace limits, search and compare.
- [What depends on what](docs/TOPOLOGY.md): the topology graph, request paths, Used by and what the delete dialog warns about.
- [Integrations](docs/INTEGRATIONS.md): Prometheus, Grafana, Loki, Flux, Argo CD, cert-manager and right-sizing, found without configuration.
- [Logs and debugging](docs/DEBUGGING.md): searching logs by field, the workspace, debug containers, node shells and the fixes Kubri offers.
- [Settings and guard rails](docs/SETTINGS.md): context names and colors, production and read-only contexts, permissions, dry-run diffs and the YAML editor.
- [Using your k9s configuration](docs/K9S.md): what Kubri reads from k9s, and what it covers.
- [Releasing](docs/RELEASING.md): how a version is published.
- [Roadmap](docs/ROADMAP.md): the steps toward k9s parity and what each one covered.
- [Kubri and Headlamp](docs/HEADLAMP-COMPARISON.md): where Headlamp does better today, and what to adopt.
- [Ideas](docs/IDEAS.md): how Kubri could go beyond Headlamp, and what neither tool has yet.

## Development

<details>
<summary>Build, run and test Kubri from source</summary>

You need Rust (stable), Node.js 22 and pnpm. On Linux the desktop app also needs the [Tauri system dependencies](https://tauri.app/start/prerequisites/).

```sh
cd ui && pnpm install

# Desktop app with hot reload
pnpm tauri dev

# Browser: run the server and the Vite dev server (which proxies /api to it)
cargo run -p kubri-server      # from the repository root
pnpm dev                       # in ui/, then open http://localhost:5173
```

To serve a production build in the browser, run `pnpm build` in `ui/`, then `cargo run --release -p kubri-server` from the repository root and open <http://127.0.0.1:7878>. On localhost it asks for no password; on other addresses it does. [Running Kubri in the browser](docs/WEB.md) has the settings, the Docker image and how to run it in a cluster.

`docker build -t kubri .` builds the image. [Releasing](docs/RELEASING.md) describes how a version is published.

Checks that CI runs:

```sh
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
cd ui && pnpm check && pnpm test
```

</details>

## License

Kubri is licensed under the [Apache License 2.0](LICENSE).
