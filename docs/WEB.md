# Running Kubri in the browser

`kubri-server` serves the same UI as the desktop app, plus the API it talks to, so Kubri runs in any browser: on your own computer, from a Docker image, or inside a cluster for a team.

Whoever can use Kubri can do everything its kubeconfig or service account allows, open shells in containers, open ports on the machine the server runs on, and run your k9s plugins there, with values from the browser such as `$FILTER` and the columns of a row. Give it a password, serve it over HTTPS when it leaves your computer, and give it only the access its users need. Without `KUBRI_LOCAL_TERMINAL=true`, nobody can run commands of their own on the server: there is no [local terminal](TERMINAL.md#in-the-web-app), and contexts added in the browser cannot run a command to sign in or read certificate and token files there.

One kubri-server is one Kubri, shared by everyone who uses it: switching the context switches it for everyone, and everyone sees and can stop the same port-forwards and benchmarks. For people who work on different clusters at the same time, run one server each.

## On your computer

Build the UI once, then start the server from the repository root:

```sh
cd ui && pnpm install && pnpm build && cd ..
cargo run --release -p kubri-server
```

Open <http://127.0.0.1:7878>. On localhost Kubri asks for no password, and it turns away requests from other websites and requests under host names other than localhost, so a web page you visit cannot use it.

## Signing in

Kubri asks for a password as soon as it listens on anything other than localhost, such as `0.0.0.0:7878`. Set one with `KUBRI_PASSWORD`, or put it in a file and point `KUBRI_PASSWORD_FILE` at it (for Docker and Kubernetes secrets). Without one, Kubri makes up a password at every start and prints it:

```text
WARN kubri_server: no password set, so Kubri made one up: 6RkzQ1vYp0dXmTq3LbNw8sJe  (set KUBRI_PASSWORD or KUBRI_PASSWORD_FILE to choose your own)
```

Signing in lasts a week (`KUBRI_SESSION_HOURS`), until you sign out, or until the server restarts. After five wrong passwords from one address, Kubri makes that address wait 30 seconds before the next try, doubling up to 16 minutes. Behind a reverse proxy every browser comes from the proxy's address, so they wait together.

| Setting | Flag | What it does |
| --- | --- | --- |
| `KUBRI_ADDR` | `--addr` | Address and port to listen on. Default `127.0.0.1:7878`; the Docker image uses `0.0.0.0:7878`. |
| `KUBRI_PASSWORD` | `--password` | The password to sign in with. Prefer the variable or the file: a flag shows up in the process list. |
| `KUBRI_PASSWORD_FILE` | `--password-file` | A file holding the password. A trailing newline is ignored. |
| `KUBRI_NO_AUTH` | `--no-auth` | `true` lets everyone in without a password, also on other addresses. Only for use behind a proxy that signs people in itself. |
| `KUBRI_ALLOWED_HOSTS` | `--allowed-hosts` | Without a password, Kubri only answers to localhost and IP addresses. Other host names it should answer to, separated by commas. |
| `KUBRI_READ_ONLY` | `--read-only` | `true` never changes the cluster: no edits, deletes, scaling, shells or dangerous plugins, whatever the k9s settings say. Lists, logs and port-forwards still work. The desktop app takes it too. |
| | `--mcp` | Serves MCP over stdin and stdout for an AI agent instead of the web UI, read-only. See [AI agents](AGENTS.md). |
| `KUBRI_LOCAL_TERMINAL` | `--local-terminal` | `true` allows the [local terminal](TERMINAL.md#in-the-web-app). It runs on the server, as the user the server runs as, so everyone who can sign in can run commands there. It also lets them [add contexts](CLUSTERS.md#adding-clusters) that sign in by running a command, like `aws`, or that name certificate and token files on the server. Off by default. |
| `KUBRI_CONFIG_DIR` | | The folder for Kubri's settings, the agents' token and their call log. Mount a volume there to keep them. |
| `KUBRI_SESSION_HOURS` | `--session-hours` | How long signing in lasts, from 1 hour to 87600 (ten years). Default 168 (a week). |
| `KUBRI_UI_DIR` | `--ui-dir` | The built UI. Default `ui/build`; the Docker image sets it. |
| `HTTPS_PROXY`, `NO_PROXY` | | A proxy to reach clusters through, and the hosts to reach without it, as for kubectl. Localhost never goes through the proxy, and a `proxy-url` in the kubeconfig wins. |
| `RUST_LOG` | | Log levels, such as `kubri_server=debug`. |

## With Docker

The image `ghcr.io/snaketech-tu/kubri` (for amd64 and arm64) has kubri-server, the UI, `helm` for rolling back and uninstalling releases, and `kubectl` for k9s plugins. To use your kubeconfig:

```sh
docker run --rm -p 127.0.0.1:7878:7878 \
  --user "$(id -u)" \
  --mount "type=bind,src=$HOME/.kube/config,dst=/kubeconfig,readonly" -e KUBECONFIG=/kubeconfig \
  -e KUBRI_PASSWORD='a long password' \
  ghcr.io/snaketech-tu/kubri
```

- `--user "$(id -u)"` lets the container read your kubeconfig, which usually only you may read.
- `KUBECONFIG` inside the container names one file, `/kubeconfig`, not a folder. The command mounts `~/.kube/config` there; `--mount` stops with an error when that file does not exist, where `-v` would mount an empty folder in its place.
- If your kubeconfigs are several files in `~/.kube`, mount the folder instead, as in [Many kubeconfig files in Docker](#many-kubeconfig-files-in-docker). If they are spread over other folders, or your kubeconfig is somewhere else, merge it into one file that only you may read and mount that: `(umask 077; kubectl config view --flatten > ~/.kube/kubri.yaml)`, then `src=$HOME/.kube/kubri.yaml`. `--flatten` copies certificate files into it too, which a mounted kubeconfig could not reach.
- In Windows PowerShell, leave out `--user`, write each line's `\` as `` ` `` and use `src=$HOME\.kube\config`.
- Kubeconfigs whose users run a command for credentials (`aws eks get-token`, `gke-gcloud-auth-plugin`, `kubelogin`) need that command in the image. Build your own image `FROM ghcr.io/snaketech-tu/kubri` that adds it, or use the desktop app.
- A cluster on your own machine, such as kind or minikube at `127.0.0.1`, is not `127.0.0.1` inside the container. On Linux, add `--network host` and drop `-p`.
- Your k9s aliases, hotkeys, plugins and skins work when you mount them: `-v "$HOME/.config/k9s:/home/kubri/.config/k9s:ro"` (on macOS the folder is `~/Library/Application Support/k9s`).
- Port-forwards open their port inside the container. To reach one, listen on `0.0.0.0` and publish the port with `-p`, or use `--network host`.

### Many kubeconfig files in Docker

When your kubeconfigs are spread over `~/.kube`, as in [this example](CLUSTERS.md#example-many-kubeconfig-files-spread-over-kube), mount the whole folder where the container's home folder has it, and leave out `KUBECONFIG`:

```sh
docker run --rm -p 127.0.0.1:7878:7878 \
  --user "$(id -u)" \
  --mount "type=bind,src=$HOME/.kube,dst=/home/kubri/.kube,readonly" \
  -e KUBRI_PASSWORD='a long password' \
  ghcr.io/snaketech-tu/kubri
```

The image then puts every kubeconfig in that folder into `KUBECONFIG` itself, with `config` first, the same way as the `kubeconfigs` function in that example, and prints the list when it starts. Nothing has to be set up on your computer for it, so the command works the same in every shell. In Windows PowerShell, use `src=$HOME\.kube` and leave out `--user`.

The files keep their places relative to each other, so certificate files they name by a relative path are found. One that names a file by its full path in your home folder, such as `/home/you/.kube/certs/ca.crt`, is not found in the container. For those, use the `--flatten` file from the list above.

When you set `KUBECONFIG` yourself, the image uses it as it is.

To build the image yourself: `docker build -t kubri .`

## In a Kubernetes cluster

Inside a pod without a kubeconfig, Kubri uses the pod's service account, as a context named `in-cluster`. [`deploy/kubernetes.yaml`](../deploy/kubernetes.yaml) sets that up in the namespace `kubri`, with read access to everything except Secrets:

```sh
kubectl apply -f deploy/kubernetes.yaml
kubectl -n kubri create secret generic kubri-password --from-literal=password='a long password'
kubectl -n kubri port-forward svc/kubri 7878:80
```

Then open <http://localhost:7878>. What Kubri may do is what the service account may do: bind the `edit` role instead of `view` to change things, open shells and see Secrets and Helm releases, or `cluster-admin` for everything. Keep one replica, since sign-ins and port-forwards live in the pod.

## Behind a reverse proxy

To reach Kubri under a domain name, put it behind a proxy or an ingress that serves HTTPS, so passwords and sessions are not sent in the clear. Kubri marks its session cookie as HTTPS-only when the proxy sends `X-Forwarded-Proto: https`.

The proxy has to pass on WebSockets, which Kubri uses for live lists, logs and terminals, and the host name the browser used, in `Host` or `X-Forwarded-Host`: Kubri compares it with the browser's `Origin` to turn away other websites. Caddy, Traefik and ingress-nginx do both by default. With nginx:

```nginx
location / {
    proxy_pass http://127.0.0.1:7878;
    proxy_http_version 1.1;
    proxy_set_header Upgrade $http_upgrade;
    proxy_set_header Connection "upgrade";
    proxy_set_header Host $host;
    proxy_set_header X-Forwarded-Proto $scheme;
    proxy_read_timeout 1h;
}
```

If the proxy signs people in itself (oauth2-proxy, an identity-aware proxy), you can start Kubri with `KUBRI_NO_AUTH=true` and list the proxy's host name in `KUBRI_ALLOWED_HOSTS`. Make sure Kubri can then only be reached through the proxy.
