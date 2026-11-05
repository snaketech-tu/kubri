# Kubri on the command line

Two of Kubri's tools also run in a terminal, without a window: the sanitizer as `kubri lint`, for CI pipelines, and [snapshots](SNAPSHOTS.md) as `kubri snapshot`. Both read your kubeconfig like the app does and change nothing in the cluster.

The desktop app's program is `kubri`; the web server's is `kubri-server`, which is also in the Docker image. Both understand the same commands:

```sh
kubri lint                      # the current context's namespace
kubri-server lint -A            # the whole cluster
docker run --rm -v ~/.kube:/home/kubri/.kube:ro ghcr.io/snaketech-tu/kubri lint -n shop
```

Where the desktop app's program is:

| System | Program |
| --- | --- |
| Linux (`.deb`, `.rpm`, AUR) | `kubri` on the `PATH` |
| macOS | `/Applications/Kubri.app/Contents/MacOS/kubri` |
| Windows | `kubri.exe` in the folder Kubri is installed in. The prompt comes back before the output is done; pipe it, as in `kubri.exe lint \| more`, or use `kubri-server`. |

## `kubri lint`

Checks a namespace, or the whole cluster with nodes, volumes and cluster roles, for problems and risky settings: crashing or pending pods, missing probes and resource requests, Services that select nothing, bindings to missing service accounts and more. It writes a report with a score from 0 to 100 and a grade from A to F, the same as the Sanitizer page.

```
$ kubri lint -n shop
prod, namespace shop: score 79, grade C

Deployment: 4 checked, 1 with warnings, score 75
  warning  shop/api  container api: No liveness or readiness probe  (probes)
  warning  shop/api  container api: No CPU or memory requests  (resources)
```

| Option | What it does |
| --- | --- |
| `--context NAME` | The context to check. The kubeconfig's current context when left out. |
| `-n`, `--namespace NS` | The namespace. The context's namespace when left out. |
| `-A`, `--all-namespaces` | The whole cluster. |
| `--format text\|json\|sarif` | Readable text (the default), the whole report as JSON, or [SARIF](https://sarifweb.azurewebsites.net/) for GitHub code scanning. |
| `-o`, `--output FILE` | Write the report to a file instead of the standard output. |
| `--fail-on error\|warning\|never` | Exit with 1 when an object has a problem of this level or worse. `error` by default. |
| `--min-score N` | Exit with 1 when the score is below `N`. |

It exits with 0 when the check passes, 1 when it does not, and 2 when it could not run, for example because the context does not exist or the cluster does not answer. Kinds that may not be listed are reported as not checked and do not count.

In SARIF, every problem names its object as `<namespace>/<kind>/<name>` (or `cluster/<kind>/<name>`), since cluster objects have no file. The check's name, such as `probes`, is the rule.

### In GitHub Actions

The `lint` action runs the Docker image against the cluster of a kubeconfig on the runner, such as a kind or k3s cluster a workflow started, or one written from a secret:

```yaml
jobs:
  lint:
    runs-on: ubuntu-latest
    permissions:
      security-events: write   # for the SARIF upload
    steps:
      - uses: actions/checkout@v4
      # … start a cluster or write a kubeconfig to ~/.kube/config, deploy your manifests …
      - uses: snaketech-tu/kubri/actions/lint@main
        with:
          namespace: shop
          fail-on: error
      - uses: github/codeql-action/upload-sarif@v3
        if: always()
        with:
          sarif_file: kubri.sarif
```

| Input | Default |
| --- | --- |
| `kubeconfig` | `KUBECONFIG`, or `~/.kube/config` |
| `context`, `namespace`, `all-namespaces`, `fail-on`, `min-score` | As the options above |
| `format` | `sarif` |
| `output` | `kubri.sarif` |
| `version` | `latest`, a tag of `ghcr.io/snaketech-tu/kubri` |

The kubeconfig is mounted alone, so it has to carry its certificates and token itself (`certificate-authority-data`, `token`), not paths to other files. A kubeconfig that signs in with a command, such as `aws eks get-token`, needs that command in the image: run `kubri-server lint` in a job that has it instead.

Elsewhere, call the program directly; GitLab CI, for example:

```yaml
kubri-lint:
  image:
    name: ghcr.io/snaketech-tu/kubri
    entrypoint: [""]
  script:
    - kubri-server lint -n shop --format json -o kubri.json
  artifacts:
    paths: [kubri.json]
```

## `kubri snapshot`

Saves a namespace, or the whole cluster with `-A`, to a snapshot file, without opening Kubri. It is the same file as **Snapshots…** in the app saves, with Secret values left out (see [Snapshots](SNAPSHOTS.md)), for a support request or an archive of a cluster's state after an incident.

```
$ kubri snapshot -n shop
prod-shop-20261104-2247.kubri
```

| Option | What it does |
| --- | --- |
| `--context NAME` | The context. The current one when left out. |
| `-n`, `--namespace NS` | The namespace. The context's namespace when left out. |
| `-A`, `--all-namespaces` | The whole cluster. |
| `-o`, `--output PATH` | The file to write, or a folder for a file named after the context, namespace and time. The current folder by default. |

It prints the path of the file it wrote.

## Opening a snapshot

`kubri prod-shop-20261104-2247.kubri`, or `kubri open FILE`, starts the desktop app with the snapshot open. `kubri-server --snapshot FILE` does the same in the browser.
