# End-to-end tests

The web app's main flows, in a real browser against a real cluster: lists and filters, context
switching and the browser's Back, logs, a shell, a port-forward and a favorite one, editing YAML, scaling, restarting
and deleting, read-only mode, snapshots, and MCP. A second check starts the desktop app on Linux
and waits for it to show the overview. CI runs both on every pull request (`.github/workflows/ci.yml`,
jobs **End-to-end** and **Desktop start**).

## Running them

You need a cluster you may change, such as k3s (see the `e2e` job in `ci.yml`), `kubectl`, a
release build of `kubri-server` and the built UI.

```sh
kubectl apply -f scripts/e2e/fixtures.yaml
kubectl rollout status deployment/web -n e2e
(cd ui && pnpm build)
cargo build --release -p kubri-server
node --test --test-reporter=spec scripts/e2e/kubri.test.cjs
```

The tests read the cluster from `KUBECONFIG` and make a copy with two contexts, `e2e-one` and
`e2e-two`. Every server they start gets settings of its own, so your Kubri and k9s settings do not
matter. When a test fails, a screenshot of the page lands in `e2e-artifacts/`.

| Variable | Default |
| --- | --- |
| `KUBRI_BIN` | `target/release/kubri-server` |
| `KUBRI_UI_DIR` | `ui/build` |
| `PLAYWRIGHT_MODULE` | `playwright` |
| `E2E_ARTIFACTS` | `e2e-artifacts` |

The desktop check needs the Tauri system libraries, `webkit2gtk-driver`, `tauri-driver`
(`cargo install tauri-driver --locked`) and Xvfb:

```sh
cargo build -p kubri-desktop --features tauri/custom-protocol
xvfb-run -a node scripts/e2e/desktop.cjs
```

## Writing one

- Reach a list by its address with `openList`, and act with the keys a user presses, as k9s
  users do: they change less often than labels and layout.
- Check the result in the cluster with `kubectl`, not only on the page.
- Create what a test changes or deletes in the test itself and remove it in `finally`.
