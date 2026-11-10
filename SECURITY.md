# Security

## Reporting a problem

Please do not open a public issue for a security problem. Report it privately instead:

1. Open the [Security tab](https://github.com/snaketech-tu/kubri/security) of this repository.
2. Click **Report a vulnerability** and describe what you found, how to make it happen and what someone could do with it.

You get an answer within a week. Once a fix is released, the advisory is published and names you, unless you would rather not be named.

## Checking a download

Every installer and every Docker image is built by the Release workflow in this repository, which signs a record of the commit it built from. With the [GitHub CLI](https://cli.github.com), check a file before you install it:

```sh
gh attestation verify Kubri_2.0.0_x64-setup.exe --owner snaketech-tu
gh attestation verify oci://ghcr.io/snaketech-tu/kubri:2.0.0 --owner snaketech-tu
```

The image also carries a list of the software in it (an SBOM), which `docker buildx imagetools inspect ghcr.io/snaketech-tu/kubri:2.0.0 --format '{{ json .SBOM }}'` shows. Releases before 2.0 have no such records.

## Supported versions

Fixes go into the newest release. Kubri updates itself on the desktop, and the Docker image tag `latest` and the Helm chart always point at the newest version, so please update before you report.

## What Kubri protects

Kubri acts on a cluster with the credentials you give it, and can do whatever those credentials allow. These are the boundaries it is meant to keep, so a way around one of them is a security problem:

- **The desktop app** listens on the network only when you turn on **Serve MCP on this computer**, and then only on `127.0.0.1` and only with its token.
- **AI agents** connected over MCP need a token, and follow the same read-only and permission rules as the app.
- **kubri-server on localhost** turns away requests from other websites and requests under other host names, so a web page you visit cannot use it.
- **kubri-server on any other address** asks for a password or an OpenID Connect sign-in. Without one of them nobody gets in.
- **Signed-in people cannot run commands on the server or read its files** unless the server runs with `KUBRI_LOCAL_TERMINAL=true`. That includes contexts added in the browser, which cannot run a sign-in command or read token and certificate files on the server.
- **With OpenID Connect**, each person can do in Kubri exactly what their own Kubernetes permissions allow, and no more.
- **Read-only** (`--read-only`, read-only contexts, k9s `readOnly`, snapshots) means no change reaches the cluster from Kubri.
- **The audit log** records every change made through kubri-server, with who made it.
- **From URL** on the Create page does not fetch from cloud metadata addresses.

[Running Kubri in the browser](docs/WEB.md#security) and [A Kubri for your team](docs/TEAMS.md) describe these settings in full.

## Not a security problem

- Someone with your kubeconfig, or with the password of a shared kubri-server, can do what its credentials allow. That is how it is meant to work: give those credentials only the rights their users need.
- k9s plugins run on the computer or server where Kubri runs, with values from the screen. Only add plugins you trust.
