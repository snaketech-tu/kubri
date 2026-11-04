# Several clusters

Kubri connects to one context at a time, the one in the top bar. The pages here look at several of them.

## Switching back is instant

When you switch to another context, Kubri keeps the one you left connected in the background, with the resource types and permissions it already found. Switching back then takes a quick check that the cluster still answers instead of a new connection. If the check fails, for example because a token expired, Kubri connects afresh and shows the error that gives. The resource types are looked up again right after a switch back, so a CRD installed meanwhile appears in the sidebar.

You also land where you left: the same page, kind, namespace, filter, row under the cursor and back and forward history. While a list reloads, it shows the rows it had when you left, dimmed and marked "updating…", and offers no actions on them until the cluster's own list arrives.

The context switcher (<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>C</kbd>) lists the previous context first, so <kbd>Enter</kbd> goes back to it, and marks kept contexts with a green dot. `:ctx -` in the command palette does the same, like `cd -`. `:ctx prod` switches to `prod`; `:ctx` alone opens Clusters, where you pick one.

While Kubri connects, the page you were on stays, dimmed, under a card with the time so far, the server's address, and **Cancel** (<kbd>Esc</kbd>), which keeps you where you were. Kubri gives up after 15 seconds. When a cluster does not answer, the card names the server and suggests checking the network or VPN, with **Try again** and **Check on the Clusters page**; for other failures, such as credentials the cluster turned down, it says what to try. The switcher shows contexts that failed lately, from connecting or from the Clusters page, such as "Unreachable, 2m ago". Opening it checks contexts not checked for five minutes, when there are at most 20.

Logs and shells in the workspace keep running when you switch. A tab from another context shows that context's name, in red for production, so a shell in production is not mistaken for one in staging. Tabs close only when their context is removed from the kubeconfig, or when you leave a snapshot.

Kubri keeps the 3 most recent contexts this way, each for up to 30 minutes after you left it. Set the number, from 0 (off) to 5, with `connections.keepRecent` in the [settings](SETTINGS.md). Nothing runs for a kept context: no lists, no recording of changes. When the kubeconfig is read again, a kept context whose context, cluster or user entry changed is dropped.

## Clusters

**Clusters** (`:fleet`, first in the sidebar) shows every context in your kubeconfig on one page, without connecting Kubri to any of them:

- whether its API server answers, its version and how long it took
- nodes, pods and workloads (Deployments, StatefulSets and DaemonSets), healthy out of all
- how much of the nodes' CPU and memory the pods request
- the name, color and label you gave the context in its settings, and whether it is production

Contexts are checked six at a time, each with its own ten-second limit, so a cluster that is down shows as **Unreachable** without holding up the others. Clusters that fail or cannot be reached are tinted red, those that need a look amber, and a line above the cards counts both. The page checks again every minute while it is open, with the connections it made the first time, so a context that signs in by running a command, like `aws`, does not run it at every check. Your user needs `list` on nodes, pods and the workload kinds across the cluster to see the counts; without it the card says the cluster is reachable but that nothing may be listed.

Clicking a cluster connects Kubri to it and opens its overview. The overview starts with what the card showed: the health line, the API server and version, ready nodes, pods and workloads, and how much CPU and memory pods request and use. When Kubri cannot connect at start, the welcome screen has **See all clusters** to find one that works.

## Adding clusters

**Add clusters…** on the Clusters page takes a kubeconfig dropped on it, chosen as a file or pasted, and lists its contexts. Pick the ones to add:

- Kubri keeps each one as its own file in `kubeconfigs` in Kubri's config folder, readable only by you. Your kubeconfig files are never changed.
- Its cluster and user are renamed after the context, so a cluster called `default` in the new file cannot clash with one of the same name in yours.
- Added contexts come after the files in `KUBECONFIG`, and Kubri hands the same list to helm and to k9s plugins, so they work there too.
- A context whose name is already taken, whose cluster or user is missing from the file, or that names a certificate or token file by a relative path cannot be added; the dialog says which. Contexts that sign in by running a command, like `aws` or `gke-gcloud-auth-plugin`, say so, since that command must be installed.
- In the browser, the command would run on the server and the files would be read there, for whoever added the context. So the server adds such contexts only when it allows the local terminal (`KUBRI_LOCAL_TERMINAL=true`, see [Running Kubri in the browser](WEB.md)). Otherwise put the file's content in the kubeconfig, as `certificate-authority-data`, `client-certificate-data`, `client-key-data` or `token`, or add the context to the server's own kubeconfig.

Added contexts have **Remove** on their card. Contexts from your own kubeconfig cannot be removed in Kubri.

## Loading kubeconfig files

**Kubeconfig files…** at the bottom of the context switcher (or `:kubeconfig`) lists every file Kubri reads, with its contexts and whatever keeps a file from being read. In the desktop app, **Choose files…** opens the system's file dialog, or type a path such as `~/.kube/lab.yaml` and press **Load**:

- Kubri reads a loaded file where it is, every time it reads the kubeconfig. Changes to the file show up when you reload the context list, and certificate or token files it names by a relative path work.
- Loaded files stay loaded when Kubri starts again. They are kept as a list of paths in `kubeconfig-files.json` in Kubri's config folder; the files themselves are never copied or changed.
- They come after the files in `KUBECONFIG`. A context that an earlier file already has keeps the earlier one, as kubectl does, and the dialog says which ones that are.
- helm and k9s plugins get them in `KUBECONFIG` too.
- **Remove** stops reading a loaded file. Your own files, from `KUBECONFIG` or `~/.kube/config`, are listed but stay.

With no kubeconfig at all, or one without contexts, the start page says so and offers **Load kubeconfig file…**. When connecting to a context fails, it says what to try for the usual causes, such as a server that does not answer or credentials the cluster turned down. In the browser, Kubri cannot open files on the server for you; use **Add contexts from a file…** in the same dialog, which copies the contexts as described above.

### Example: many kubeconfig files spread over `~/.kube`

Say every team or tool leaves its own file somewhere under `~/.kube`:

```text
~/.kube/
├── config                      # your own contexts
├── clusters/prod/eks.yaml      # names ca.crt next to it
├── clusters/prod/ca.crt
├── kind/kind-config
├── teams/shop/dev.yml
├── cache/ …                    # kubectl's caches, not kubeconfigs
└── kubectx                     # some other tool's file
```

To have all of them in Kubri, and in kubectl, helm and k9s too, put them in `KUBECONFIG`. On Linux and macOS, this shell function lists every file under a folder that is a kubeconfig, with `config` first, so its contexts and its current context win:

```sh
# Every kubeconfig under ~/.kube, as a KUBECONFIG list. The paths start with $1.
kubeconfigs() (
  cd ~/.kube &&
  { echo config; grep -rl --exclude-dir=cache --exclude-dir=http-cache '^kind: Config' . | sed 's|^\./||' | sort; } |
    awk '!seen[$0]++' | sed "s|^|$1/|" | paste -sd: -
)
export KUBECONFIG="$(kubeconfigs "$HOME/.kube")"
```

For the example above that gives `~/.kube/config:~/.kube/clusters/prod/eks.yaml:~/.kube/kind/kind-config:~/.kube/teams/shop/dev.yml`, with your home folder spelled out.

- Put both in `~/.bashrc` or `~/.zshrc`. The desktop app reads the environment of your login shell when it starts, so it gets the list even when started from the menu or dock, and a new file shows up the next time Kubri starts.
- A file counts when it has a line `kind: Config`, as every file kubectl writes does. The caches and other tools' files are left out. Load a file without that line with **Choose files…**.
- When two files have a context of the same name, the earlier one wins, as in kubectl. **Kubeconfig files…** says which contexts that hides.
- A file that is not valid YAML stops kubectl and Kubri from reading the whole list. **Kubeconfig files…** says which file it is.
- Paths in a file relative to it, such as `certificate-authority: ca.crt`, keep working, since every file is read where it is.

Without the shell, the same works with **Choose files…** in **Kubeconfig files…**: select several files of a folder at once, then do the same for the next folder. Kubri remembers them, but a new file has to be loaded too.

In the browser, [kubri-server in Docker](WEB.md#many-kubeconfig-files-in-docker) can mount the whole folder and finds the files in it by itself, without the function.

## Limiting namespaces

In a context's settings (`:config`), **Only these namespaces** takes a list such as `shop, payments`, and **Or namespaces labeled** a label selector such as `team=shop`. The namespace picker then offers only those, and lists across all namespaces show only them, with one watch per namespace. That also suits users who may list only some namespaces. A namespace you pick or type yourself still opens.

## Search

**Search** (`:search`, also `:find`) finds objects by name, label and field, in the connected context or in several. Type part of a name and narrow it down:

| Part | Means |
| --- | --- |
| `web` | the name contains `web`; exact names come first, then names that start with it |
| `/^api-\d+$/` | the name matches a regular expression, ignoring case |
| `kind:deploy,svc` | these kinds, named as in the palette; `kind:all` every kind you may list |
| `ns:shop` or `-n shop` | one namespace |
| `-l app=web` or `label:app=web` | a label selector |
| `status.phase!=Running` | a field compared with `=`, `!=`, `~` (contains), `!~`, `>`, `<`, `>=` or `<=`; numbers compare as numbers |
| `ctx:*` or `ctx:prod,staging` | every context, or some; without it only the connected one |

Without `kind:` Kubri searches the kinds people look for by name: workloads, pods, Services, Ingresses, ConfigMaps, Secrets, volume claims, ServiceAccounts, nodes, namespaces and HTTPRoutes. A context limited to some namespaces is searched in those. Lists that fail, such as a context that is down, are named above the results and do not hold up the others. Up to 300 objects are shown.

Clicking a result opens it, connecting to its context first when it is another one. **Save search** puts the search in the sidebar under Search, kept in Kubri's settings file.

The palette (`:` or `Ctrl K`) also finds objects: it offers the objects opened lately, in any context, and while you type a word of three letters or more it looks for objects in the connected context whose name has it. **search …** at the end of the list opens the search page with what you typed.

## Compare

**Compare…** in an object's menu puts it beside the same object in another context or namespace, by default the next context with the same namespace and name. It shows the containers whose images differ, then the differences as a diff, for example a Deployment in staging and in production.

What each copy keeps of its own is left out: the namespace, `uid`, `resourceVersion`, `generation`, timestamps, managed fields, `last-applied-configuration`, the Deployment revision and the restart stamp of `kubectl rollout restart`. **Include status** compares the status too.

Kubri reads the other object with the other context's credentials. The connection is kept for five minutes, so comparing again is quick.
