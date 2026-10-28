# Running Kubri in the browser

`kubri-server` serves the same UI as the desktop app, plus the API it talks to, so Kubri runs in any browser: on your own computer, from a Docker image, or inside a cluster for a team.

Whoever can use Kubri can do everything its kubeconfig or service account allows, open shells in containers, open ports on the machine the server runs on, and run your k9s plugins there. Give it a password, serve it over HTTPS when it leaves your computer, and give it only the access its users need.

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
| `KUBRI_SESSION_HOURS` | `--session-hours` | How long signing in lasts. Default 168 (a week). |
| `KUBRI_UI_DIR` | `--ui-dir` | The built UI. Default `ui/build`; the Docker image sets it. |
| `RUST_LOG` | | Log levels, such as `kubri_server=debug`. |

## With Docker

The image `ghcr.io/snaketech-tu/kubri` (for amd64 and arm64) has kubri-server, the UI, `helm` for rolling back and uninstalling releases, and `kubectl` for k9s plugins. To use your kubeconfig:

```sh
docker run --rm -p 127.0.0.1:7878:7878 \
  --user "$(id -u)" \
  -v "$HOME/.kube/config:/kubeconfig:ro" -e KUBECONFIG=/kubeconfig \
  -e KUBRI_PASSWORD='a long password' \
  ghcr.io/snaketech-tu/kubri
```

- `--user "$(id -u)"` lets the container read your kubeconfig, which usually only you may read.
- Kubeconfigs whose users run a command for credentials (`aws eks get-token`, `gke-gcloud-auth-plugin`, `kubelogin`) need that command in the image. Build your own image `FROM ghcr.io/snaketech-tu/kubri` that adds it, or use the desktop app.
- A cluster on your own machine, such as kind or minikube at `127.0.0.1`, is not `127.0.0.1` inside the container. On Linux, add `--network host` and drop `-p`.
- Your k9s aliases, hotkeys, plugins and skins work when you mount them: `-v "$HOME/.config/k9s:/home/kubri/.config/k9s:ro"` (on macOS the folder is `~/Library/Application Support/k9s`).
- Port-forwards open their port inside the container. To reach one, listen on `0.0.0.0` and publish the port with `-p`, or use `--network host`.

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
