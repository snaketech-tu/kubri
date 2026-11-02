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
    <td width="33%" valign="top"><b>Everything k9s does</b><br>Every resource and CRD, live: drill-downs, logs, shells, port-forwards, Pulses, XRay, RBAC, Helm, the sanitizer and benchmarks. Your k9s aliases, hotkeys, plugins and skins keep working.</td>
    <td width="33%" valign="top"><b>Keyboard or mouse</b><br>The keys you know from k9s: <kbd>:</kbd> opens the command palette and <kbd>?</kbd> lists every key. Or click: each action is a button, and a right click on a row shows all of them with their keys.</td>
    <td width="33%" valign="top"><b>Desktop or browser</b><br>A small native app built with Rust and Tauri, a container for your team with a password sign-in, or a pod in the cluster with its own service account.</td>
  </tr>
</table>

## Install

### Desktop app

Download Kubri for Windows, macOS or Linux from the [releases](https://github.com/snaketech-tu/kubri/releases). It uses the kubeconfig on your computer, just like kubectl, and updates itself when a new version is out.

### In the browser

```sh
docker run --rm -p 127.0.0.1:7878:7878 \
  --user "$(id -u)" \
  --mount "type=bind,src=$HOME/.kube/config,dst=/kubeconfig,readonly" -e KUBECONFIG=/kubeconfig \
  -e KUBRI_PASSWORD='a long password' \
  ghcr.io/snaketech-tu/kubri
```

Then open <http://127.0.0.1:7878> and sign in with the password. If your `KUBECONFIG` lists several files, or you use Windows, see [With Docker](docs/WEB.md#with-docker).

### In a cluster

```sh
kubectl apply -f https://raw.githubusercontent.com/snaketech-tu/kubri/main/deploy/kubernetes.yaml
kubectl -n kubri create secret generic kubri-password --from-literal=password='a long password'
kubectl -n kubri port-forward svc/kubri 7878:80
```

Kubri then sees what its service account may see. [Running Kubri in the browser](docs/WEB.md#in-a-kubernetes-cluster) explains how to give it more rights and put it behind a proxy.

## Features

<table>
  <tr>
    <td width="50%" valign="top">
      <b>Browse</b>
      <ul>
        <li>Every resource type and CRD, live, with the columns of <code>kubectl get</code></li>
        <li>k9s filters: regular expressions, <code>!</code> to invert, <code>-f</code> fuzzy, <code>-l app=web</code></li>
        <li>Drill down with <kbd>Enter</kbd>, jump to the owner, go back and forth</li>
        <li>CPU and memory against requests, limits and node capacity</li>
        <li>Your own columns per kind, JSONPath included</li>
      </ul>
    </td>
    <td width="50%" valign="top">
      <b>Change</b>
      <ul>
        <li>Create from templates, forms, files or the clipboard (<kbd>:new</kbd>)</li>
        <li>Delete, scale, restart, set images and roll back, one object or many</li>
        <li>Edit YAML with completion and field docs from the cluster's schema</li>
        <li>Every change as a server-side dry run with a diff first</li>
        <li>Clone objects to another namespace or cluster; cordon and drain nodes</li>
      </ul>
    </td>
  </tr>
  <tr>
    <td valign="top">
      <b>Debug</b>
      <ul>
        <li>Live logs of a pod or a whole workload, with JSON fields as columns and filters like <code>status>=500</code></li>
        <li>Shells, debug containers and node shells in a real terminal</li>
        <li>Logs and shells that keep running in a workspace while you browse</li>
        <li>The likely problem of a pod, with a fix to apply</li>
        <li>Port-forwards that survive pod restarts, with HTTP benchmarks</li>
      </ul>
    </td>
    <td valign="top">
      <b>Understand the cluster</b>
      <ul>
        <li>Pulses, XRay, a topology graph and an RBAC matrix</li>
        <li>Helm: install, upgrade with a diff, compare revisions, roll back</li>
        <li>Prometheus, Grafana, Loki, Flux, Argo CD and cert-manager, found without setup</li>
        <li>What changed: rollouts, scaling and config changes as a timeline with diffs</li>
        <li>All your clusters on one page, with search across them</li>
        <li>Snapshots of a namespace or cluster to open offline</li>
      </ul>
    </td>
  </tr>
  <tr>
    <td valign="top">
      <b>Work safely</b>
      <ul>
        <li>A name and color per context, and a banner on production</li>
        <li>Read-only and locked contexts</li>
        <li>Missing permissions explained, with a Role to ask for</li>
        <li>What breaks before you delete something still in use</li>
        <li>Every action as the matching <code>kubectl</code> command</li>
      </ul>
    </td>
    <td valign="top">
      <b>Fits your setup</b>
      <ul>
        <li>Your k9s configuration, per context</li>
        <li>Views in YAML that give your CRDs columns, health and actions</li>
        <li>Fast with big clusters: 10,000 pods in about a second, <a href="docs/SPEED.md">measured in CI</a></li>
        <li>An MCP server so AI agents can read your clusters with your permissions</li>
        <li>Explain: ask your own model why a pod fails, and see first what is sent</li>
      </ul>
    </td>
  </tr>
</table>

## Screenshots

<table>
  <tr>
    <td width="50%" valign="top"><img src="docs/assets/screenshots/overview.png" alt="Cluster overview: pods and nodes, CPU and memory, and the objects that need attention"><br><b>Overview</b>: what needs attention first, with health and trends (<kbd>:pulses</kbd>)</td>
    <td width="50%" valign="top"><img src="docs/assets/screenshots/describe.png" alt="A crash-looping pod in the inspector beside the list"><br><b>Inspector</b>: status, the likely problem, events and previous logs beside the list</td>
  </tr>
  <tr>
    <td width="50%" valign="top"><img src="docs/assets/screenshots/logs.png" alt="A Deployment's merged logs as a table of JSON fields, filtered by status>=500"><br><b>Structured logs</b>: fields as columns, filters like <code>status>=500</code>, and a rate strip</td>
    <td width="50%" valign="top"><img src="docs/assets/screenshots/workspace.png" alt="The pod list with a shell and a Deployment's logs side by side in the workspace below"><br><b>Workspace</b>: logs and shells that keep running while you browse</td>
  </tr>
  <tr>
    <td width="50%" valign="top"><img src="docs/assets/screenshots/xray.png" alt="Topology: deployments down to their replica sets and pods"><br><b>Topology</b>: from a Deployment down to its pods (<kbd>:xray</kbd>)</td>
    <td width="50%" valign="top"><img src="docs/assets/screenshots/safety.png" alt="Setting an image on a production context: the banner, and the change as the API server would store it"><br><b>Production guard rails</b>: a colored context, and every change as a dry-run diff first</td>
  </tr>
</table>

<details>
<summary>More screenshots</summary>

<table>
  <tr>
    <td width="50%" valign="top"><img src="docs/assets/screenshots/palette.png" alt="The command palette suggesting resources"><br><b>Command palette</b>: <kbd>:</kbd> and any resource or command</td>
    <td width="50%" valign="top"><img src="docs/assets/screenshots/helm.png" alt="A Helm release with its objects, notes and revisions"><br><b>Helm</b>: releases, values, manifests, history and rollback</td>
  </tr>
  <tr>
    <td width="50%" valign="top"><img src="docs/assets/screenshots/rbac.png" alt="Access: a service account's role bindings and the rules they grant"><br><b>Access</b>: who may do what, from the role bindings (<kbd>:rbac</kbd>)</td>
    <td width="50%" valign="top"><img src="docs/assets/screenshots/editor.png" alt="Editing a Deployment's YAML with completion and field docs from the cluster's schema"><br><b>YAML editor</b>: completion and field docs, like <code>kubectl explain</code></td>
  </tr>
  <tr>
    <td width="50%" valign="top"><img src="docs/assets/screenshots/explain.png" alt="Explain beside a crash-looping pod: the likely cause and how to fix it"><br><b>Explain</b>: ask your own model why a pod fails, after seeing what is sent</td>
    <td width="50%" valign="top"><img src="docs/assets/screenshots/agents.png" alt="Settings for AI agents: the MCP endpoint, the command for Claude Code and the log of tool calls"><br><b>AI agents</b>: Kubri as an MCP server, with every call logged (<kbd>:agents</kbd>)</td>
  </tr>
  <tr>
    <td width="50%" valign="top"><img src="docs/assets/screenshots/create.png" alt="Create with a Deployment's YAML and a form for its name, replicas, image, port and resources"><br><b>Create</b>: templates, short forms like <code>:new deploy web --image nginx</code> and forms that write the YAML</td>
    <td width="50%" valign="top"><img src="docs/assets/screenshots/helm-upgrade.png" alt="A Helm upgrade review: the changed values and the Deployment whose replicas and image change"><br><b>Helm upgrade</b>: change values and see every object that changes before you upgrade</td>
  </tr>
  <tr>
    <td width="50%" valign="top"><img src="docs/assets/screenshots/metrics.png" alt="A Deployment's CPU and memory per pod from Prometheus, against its request and limit, with a link to its Grafana dashboard"><br><b>Metrics</b>: Prometheus history against requests and limits, and the matching Grafana dashboard</td>
    <td width="50%" valign="top"><img src="docs/assets/screenshots/right-sizing.png" alt="Right-sizing: containers whose requests are far from their use, with suggested values"><br><b>Right-sizing</b>: what each container requests and uses, and what to set instead (<kbd>:rightsizing</kbd>)</td>
  </tr>
  <tr>
    <td width="50%" valign="top"><img src="docs/assets/screenshots/fleet.png" alt="Clusters: a card per context with its health, nodes, pods, workloads and requests, including unreachable ones"><br><b>Clusters</b>: every context side by side, even when some are down (<kbd>:fleet</kbd>)</td>
    <td width="50%" valign="top"><img src="docs/assets/screenshots/search.png" alt="Search for web across two contexts, listing Deployments, Services and Ingresses"><br><b>Search</b>: objects by name, label and field in several clusters at once (<kbd>:search</kbd>)</td>
  </tr>
  <tr>
    <td width="50%" valign="top"><img src="docs/assets/screenshots/graph.png" alt="Topology graph of a namespace: Ingresses, Services, a Deployment with its Secret and pod, and request paths with two broken links"><br><b>Topology graph</b>: how a namespace connects, and where a request path breaks (<kbd>:graph</kbd>)</td>
    <td width="50%" valign="top"><img src="docs/assets/screenshots/delete.png" alt="Deleting a Secret: the dialog warns it is used by a Deployment and by an Ingress for TLS"><br><b>Blast radius</b>: what breaks if you delete it, before you confirm</td>
  </tr>
  <tr>
    <td width="50%" valign="top"><img src="docs/assets/screenshots/changes.png" alt="What changed in a namespace: a ConfigMap's annotations, a Deployment's new environment variable shown as a diff, and a scale from 2 to 3, with the times Kubri was not watching"><br><b>What changed</b>: rollouts, scaling and config changes with their diffs (<kbd>:changes</kbd>)</td>
    <td width="50%" valign="top"><img src="docs/assets/screenshots/snapshot.png" alt="A snapshot of a namespace opened read-only, with a banner naming its context and time, showing the cluster overview"><br><b>Snapshots</b>: what you see, in one file anyone can open offline (<kbd>:snapshot</kbd>)</td>
  </tr>
  <tr>
    <td width="50%" valign="top"><img src="docs/assets/screenshots/views.png" alt="A custom resource list with Color, Size and Health columns from a view file, and its details with the view's health and Build section"><br><b>Views</b>: columns, health and details for any CRD from a YAML file (<kbd>:views</kbd>)</td>
    <td width="50%" valign="top"></td>
  </tr>
</table>

</details>

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

## Your kubeconfig

Kubri reads the files in `KUBECONFIG`, or `~/.kube/config`, the way kubectl does: a context that works with kubectl works in Kubri, and one that fails shows kubectl's error. In the desktop app you can load more kubeconfig files under **Kubeconfig files…** in the context switcher, and Kubri remembers them. If your kubeconfigs are spread over `~/.kube`, [this example](docs/CLUSTERS.md#example-many-kubeconfig-files-spread-over-kube) puts them all in `KUBECONFIG`. It starts with your `current-context`, and you switch contexts in the top bar or with <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>C</kbd>. Started from a desktop launcher, the app takes the environment of your login shell, so `helm`, credential plugins such as `aws` or `gke-gcloud-auth-plugin` and k9s plugins are found as in a terminal.

## Documentation

**Using Kubri**

- [Creating and changing](docs/CREATE.md): Create, forms, cloning, Helm, columns, bulk changes and port-forwards
- [Logs and debugging](docs/DEBUGGING.md): log search, the workspace, debug containers and suggested fixes
- [Local terminal](docs/TERMINAL.md): a shell with `kubectl` and `helm` set to the context and namespace Kubri shows
- [Several clusters](docs/CLUSTERS.md): the Clusters page, loading kubeconfig files, added contexts, search and compare
- [What depends on what](docs/TOPOLOGY.md): the topology graph, request paths and delete warnings
- [What changed](docs/CHANGES.md) and [Snapshots](docs/SNAPSHOTS.md)
- [Settings and guard rails](docs/SETTINGS.md): context colors, production and read-only contexts, permissions
- [Integrations](docs/INTEGRATIONS.md): Prometheus, Grafana, Loki, Flux, Argo CD, cert-manager and right-sizing
- [Views for custom resources](docs/VIEWS.md) and [your k9s configuration](docs/K9S.md)
- [AI agents (MCP)](docs/AGENTS.md) and [Explain](docs/EXPLAIN.md)

**Running Kubri**

- [In the browser](docs/WEB.md): settings, sign-in, Docker, Kubernetes and reverse proxies
- [Speed](docs/SPEED.md): how fast Kubri is with 10,000 pods, and how to measure it yourself

## Development

<details>
<summary>Build, run and test Kubri from source</summary>

| Part | Path | What it is |
| --- | --- | --- |
| Core | [`crates/kubri-core`](crates/kubri-core) | All Kubernetes access, built on [kube-rs](https://kube.rs), shared by both apps |
| Web server | [`crates/kubri-server`](crates/kubri-server) | Serves the UI and a REST and WebSocket API |
| Desktop app | [`ui/src-tauri`](ui/src-tauri) | [Tauri 2](https://tauri.app) shell that calls the core directly |
| UI | [`ui`](ui) | SvelteKit and TypeScript, used by both |

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
