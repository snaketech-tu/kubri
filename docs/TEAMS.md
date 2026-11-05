# A Kubri for your team

One kubri-server that everyone on a team opens in their browser. People sign in with your OpenID Connect provider, such as Keycloak, Dex, Entra ID, Google, Okta or GitLab, and each person works with their own Kubernetes permissions: someone who may only read the `shop` namespace sees and changes in Kubri exactly what `kubectl` would let them. Every change is written to an audit log.

- [How it works](#how-it-works)
- [Install it with Helm](#install-it-with-helm)
- [Give people their rights](#give-people-their-rights)
- [Passing on people's tokens](#passing-on-peoples-tokens)
- [The audit log](#the-audit-log)
- [Without Helm](#without-helm)
- [Settings](#settings)
- [What is different on a shared Kubri](#what-is-different-on-a-shared-kubri)

## How it works

1. Someone opens Kubri and clicks **Sign in with** your provider. The provider signs them in and sends them back to Kubri.
2. Kubri checks the ID token the provider gave (its signature, issuer, audience, expiry and nonce) and takes the person's name from its `email` claim and their groups from its `groups` claim.
3. Kubri makes a Kubri of their own for that browser: their own connection, lists, port-forwards and terminals. Switching contexts switches only theirs.
4. Every request they make reaches the cluster as them, in one of two ways (`team.credentials` in the chart, `KUBRI_TEAM_CREDENTIALS` on the server):
   - **`impersonate`** (the default) works with every cluster. Kubri uses its own credentials, usually its service account, and impersonates the person, as `kubectl --as=alice@example.com --as-group=devs` does. Kubernetes checks the person's RoleBindings, and its own audit log names them as the impersonated user. Kubri's service account needs the right to impersonate, and nothing else.
   - **`token`** passes on the person's own ID token. The cluster's API server must accept tokens from the same provider (see [Passing on people's tokens](#passing-on-peoples-tokens)). Kubri's own credentials are not used for people's requests at all.

Kubri renews the person's tokens with the provider shortly before they expire. When it cannot, because the person was removed or the provider gave no refresh token, the person signs in again. Signing in lasts a week at most (`KUBRI_SESSION_HOURS`).

## Install it with Helm

The chart is in [`deploy/helm/kubri`](../deploy/helm/kubri). First register Kubri with your provider:

- a client (application) of the kind "web application" or "confidential client", with a client secret,
- the redirect URI `https://kubri.example.com/api/auth/callback`, with your host name,
- the scopes `openid email profile groups`, and `offline_access` if your provider wants it for refresh tokens,
- a `groups` claim in the ID token. Most providers need this switched on, and some call it differently (see `oidc.groupsClaim`).

Then install it from a clone of this repository:

```sh
helm install kubri deploy/helm/kubri -n kubri --create-namespace \
  --set auth=oidc \
  --set oidc.issuer=https://login.example.com/realms/main \
  --set oidc.clientId=kubri \
  --set oidc.clientSecret='the client secret' \
  --set ingress.enabled=true \
  --set ingress.host=kubri.example.com \
  --set ingress.tlsSecret=kubri-tls
```

- `oidc.issuer` is the issuer URL exactly as the provider names itself, the address before `/.well-known/openid-configuration`.
- Instead of `oidc.clientSecret`, put the secret in a Kubernetes secret under the key `client-secret` and name it in `oidc.existingSecret`.
- If the provider's HTTPS certificate is signed by your own certificate authority, put it in a ConfigMap under `ca.crt` and name it in `oidc.caConfigMap`.
- `team.activityGroups` lists the groups whose members see everyone's changes on the Activity panel, such as `{platform-team}`.
- The chart writes the audit log to the pod's log (`auditLog: "-"`).

`helm install` prints the redirect URI to register and how to read the audit log. Without an ingress, set `publicUrl` to the address people open Kubri at, so the provider sends them back there. The chart runs one replica: sign-ins and each person's connection live in the pod, so restarting it signs everyone out.

With `auth=password` instead, the chart sets up the Kubri that [Running Kubri in the browser](WEB.md#in-a-kubernetes-cluster) describes: one password, and everyone acts as Kubri's service account with the ClusterRole in `rbac.clusterRole`. All values are explained in [`values.yaml`](../deploy/helm/kubri/values.yaml).

## Give people their rights

Kubri gives nobody rights; the cluster's RBAC does. Bind roles to the names and groups from your provider, as you would for kubectl:

```sh
# Everyone in the group devs may look at the namespace shop.
kubectl create rolebinding devs-view -n shop --clusterrole=view --group=devs
# The platform team may change everything.
kubectl create clusterrolebinding platform-admin --clusterrole=cluster-admin --group=platform-team
```

To see what someone may do: `kubectl auth can-i --list -n shop --as=alice@example.com --as-group=devs`. In Kubri, an action a person may not take says which permission is missing, such as "not allowed: needs `patch` on `deployments.apps/scale` in `shop`".

If your cluster's RoleBindings use prefixed names, such as `oidc:alice@example.com`, set `oidc.usernamePrefix` and `oidc.groupsPrefix` to the same prefix. Names starting with `system:` are refused, and groups starting with `system:` are left out, because Kubernetes keeps those for itself. `oidc.allowedGroups` lets only members of some groups sign in at all, by the group names the provider sends, without a prefix.

## Passing on people's tokens

With `team.credentials=token`, the cluster's API server must trust your provider for the same client ID. For a cluster you run yourself, that is the API server's flags:

```text
--oidc-issuer-url=https://login.example.com/realms/main
--oidc-client-id=kubri
--oidc-username-claim=email
--oidc-groups-claim=groups
```

or the same in a structured authentication configuration. Managed clusters call this an OIDC identity provider for the cluster. With token mode, the chart gives Kubri's service account no rights at all.

In token mode the API server decides who a person is from the token. Kubri's `oidc.usernameClaim` and prefixes then only name the person on the screen and in the audit log, so set them the way the API server does, or the two logs name people differently.

ID tokens are short-lived. Kubri renews them, and needs a refresh token for that, so ask for `offline_access` where the provider requires it. Kubri keeps each person's token in a file only the server's user can read, under the system's temporary folder, and deletes it within a minute after the person signs out or their session ends.

## The audit log

Every change made through Kubri is one JSON line in the audit log: edits, deletes, scaling, restarts, new images, rollbacks, Helm installs, upgrades and uninstalls, shells, port-forwards, cordoning and draining nodes, changes to settings, and every other change Kubri can make. Tries that failed are written too, with the reason. Lists, logs and other reading are not written, and neither are dry runs.

```json
{"time":"2026-11-05T00:36:28.040Z","person":"bob@example.com","groups":["admins"],"client":"10.0.3.17","context":"in-cluster","action":"scale","target":{"resource":"deployments.apps","namespace":"shop","name":"frontend"},"detail":{"replicas":2},"ok":true}
{"time":"2026-11-05T00:36:20.088Z","person":"alice@example.com","groups":["devs"],"client":"10.0.3.21","context":"in-cluster","action":"scale","target":{"resource":"deployments.apps","namespace":"shop","name":"frontend"},"detail":{"replicas":2},"ok":false,"error":"not allowed: needs `patch` on `deployments.apps/scale` in `shop`"}
```

Set `KUBRI_AUDIT_LOG` to a file, or to `-` for the server's standard output (the container's log; Kubri's own messages go to standard error). Kubri adds to the file and never truncates it. Behind a reverse proxy, `client` is the proxy's address.

The **Team** tab of the Activity panel shows the newest thousand changes since the server started: each person's own, and everyone's to members of `KUBRI_ACTIVITY_GROUPS`.

The audit log also works on a Kubri without OpenID Connect. There nobody is signed in by name, so the entries name no person, and everyone sees all of them on the Activity panel.

## Without Helm

Run `kubri-server` with at least these variables (the [Docker image](WEB.md#with-docker) works too):

```sh
KUBRI_ADDR=0.0.0.0:7878 \
KUBRI_OIDC_ISSUER=https://login.example.com/realms/main \
KUBRI_OIDC_CLIENT_ID=kubri \
KUBRI_OIDC_CLIENT_SECRET_FILE=/run/secrets/kubri-client-secret \
KUBRI_PUBLIC_URL=https://kubri.example.com \
KUBRI_AUDIT_LOG=/var/log/kubri/audit.jsonl \
kubri-server
```

Kubri then uses the contexts of its kubeconfig, or the pod's service account in a cluster. For `impersonate`, the credentials of each context need the right to impersonate users and groups:

```yaml
apiVersion: rbac.authorization.k8s.io/v1
kind: ClusterRole
metadata:
  name: kubri-impersonate
rules:
  - apiGroups: [""]
    resources: [users, groups]
    verbs: [impersonate]
```

Whoever holds those credentials can act as anyone, so keep them, the server and its namespace as safe as a cluster admin's credentials. Serve Kubri over HTTPS, as in [Behind a reverse proxy](WEB.md#behind-a-reverse-proxy).

## Settings

| Setting | Flag | What it does |
| --- | --- | --- |
| `KUBRI_OIDC_ISSUER` | `--oidc-issuer` | The provider's issuer URL. Turns on signing in with OpenID Connect; it cannot be combined with a password, `KUBRI_NO_AUTH` or the local terminal. |
| `KUBRI_OIDC_CLIENT_ID` | `--oidc-client-id` | Kubri's client ID at the provider. |
| `KUBRI_OIDC_CLIENT_SECRET` | `--oidc-client-secret` | Kubri's client secret. Leave it out for a public client. |
| `KUBRI_OIDC_CLIENT_SECRET_FILE` | `--oidc-client-secret-file` | A file holding the client secret instead. |
| `KUBRI_OIDC_NAME` | `--oidc-name` | The name on the sign-in button. The issuer's host name by default. |
| `KUBRI_OIDC_SCOPES` | `--oidc-scopes` | The scopes to ask for. By default `openid email profile`, plus `groups` and `offline_access` where the provider has them. |
| `KUBRI_OIDC_USERNAME_CLAIM` | `--oidc-username-claim` | The claim that names a person. Default `email`; Kubri then refuses addresses the provider says it has not verified. |
| `KUBRI_OIDC_USERNAME_PREFIX` | `--oidc-username-prefix` | Put before every name, such as `oidc:`. |
| `KUBRI_OIDC_GROUPS_CLAIM` | `--oidc-groups-claim` | The claim with a person's groups. Default `groups`. |
| `KUBRI_OIDC_GROUPS_PREFIX` | `--oidc-groups-prefix` | Put before every group. |
| `KUBRI_OIDC_ALLOWED_GROUPS` | `--oidc-allowed-groups` | Only members of these groups may sign in, separated by commas. |
| `KUBRI_OIDC_CA_FILE` | `--oidc-ca-file` | A PEM file with the provider's certificate authority, if the system does not trust it. |
| `KUBRI_TEAM_CREDENTIALS` | `--team-credentials` | `impersonate` (default) or `token`, see [How it works](#how-it-works). |
| `KUBRI_PUBLIC_URL` | `--public-url` | The address people open Kubri at, for the provider to send them back to. Taken from each request when not set. |
| `KUBRI_AUDIT_LOG` | `--audit-log` | A file for the audit log, or `-` for standard output. |
| `KUBRI_ACTIVITY_GROUPS` | `--activity-groups` | Groups whose members see everyone's changes on the Activity panel. |
| `KUBRI_SESSION_HOURS` | `--session-hours` | How long signing in lasts at most. Default 168 (a week). |

## What is different on a shared Kubri

- **Contexts** come from the server's kubeconfig or its service account. Nobody can add contexts or kubeconfig files in the browser.
- **Settings**, such as context colors and locks, integrations and saved searches, are shared by everyone. A change to them changes them for everyone, and it is in the audit log.
- **Not available**: k9s plugins, the local terminal and AI agents (MCP), since they would run on the server or act with its credentials rather than as the person.
- **Port-forwards** open their port on the server, as on every web Kubri. Each person sees and stops only their own.
- `KUBRI_READ_ONLY=true` still applies to everyone, whatever their rights.
