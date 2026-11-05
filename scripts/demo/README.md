# The demo

Kubri comes with a demo: `ui/static/demo.kubri`, a [snapshot](../../docs/SNAPSHOTS.md) of a small shop. The start page opens it with **Open the demo** when there is no kubeconfig, and the website shows it as a live demo in the browser.

## Making the snapshot again

`shop.yaml` is the shop: Deployments with structured logs, a StatefulSet, a CronJob, an Ingress, an HPA, a PodDisruptionBudget, RBAC, and a few things that go wrong on purpose (a crash loop, an image that cannot be pulled, a pod that cannot be scheduled). `search/` is a small Helm chart, installed twice so the release has two revisions.

Against a cluster you may change, such as k3s with `--disable=traefik`:

```sh
cargo build --release -p kubri-server   # and `pnpm build` in ui/
scripts/demo/make.sh                    # applies the shop, waits 4 minutes, saves ui/static/demo.kubri
```

It renames the context to `demo` in a copy of your kubeconfig, rolls `frontend` out once more so it has a rollout history, and waits for logs, restarts and runs of the CronJob (`DEMO_WAIT`, in seconds). Commit the new `ui/static/demo.kubri`; a test in `kubri-core` checks that it still opens.

## The live demo

The live demo is the web app without kubri-server. `record.cjs` starts kubri-server with the snapshot, looks at every page, list and object in a browser, and saves what the server answered. A build with `VITE_KUBRI_DEMO` set to that file answers from it instead (`ui/src/lib/api/replay.ts`); what was not recorded, like a filter on a log, says that it is not part of the demo.

```sh
node scripts/demo/record.cjs <path-to-playwright-module> /tmp/demo-recording.json
cd ui && KUBRI_BASE_PATH=/demo VITE_KUBRI_DEMO=demo-recording.json pnpm build
cp /tmp/demo-recording.json build/
```

The Website workflow (`.github/workflows/pages.yml`) does this, puts `site/` in front and publishes both to GitHub Pages after each release.
