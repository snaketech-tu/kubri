# Speed

How fast Kubri opens a big cluster, measured on every pull request. The CI job **Speed** runs Kubri against a k3s cluster with 10,000 pods and fails when it gets slower or bigger than its limits.

## The numbers

From CI on 2026-10-30 (GitHub's `ubuntu-latest`, 4 CPUs), the median of 3 runs:

| | Kubri | Kubri limit |
| --- | --- | --- |
| Server start | 10 ms | 500 ms |
| Page open until the first pods show | 1.12 s | 3.00 s |
| Page open until all 10,000 pods are listed | 1.12 s | 5.00 s |
| Server memory, peak | 285 MB | 400 MB |
| Page memory (JavaScript heap) | 10 MB | 100 MB |

Every run's numbers are in the job summary and in the `speed` artifact (`speed.json`).

What they mean:

- **All 10,000 pods are listed at once,** about a second after the page opens.
- **The server holds the lists.** It keeps a watched copy of each list it shows, which is what makes sorting, filters, Pulses and search instant and live, and is most of its memory.
- **The page stays small** because the list draws only the rows in view, whether there are 300 or 10,000.

## How it is measured

- [`bench/speed/seed.sh`](../bench/speed/seed.sh) creates the pods in the namespace `bench`. They name a scheduler that does not exist, so they stay Pending and need no room on the node.
- [`bench/speed/run.cjs`](../bench/speed/run.cjs) starts Kubri's server and opens its pod list in Chromium with Playwright, at 1440×900:
  - **Server start**: from starting the process until it answers its first HTTP request.
  - **First pods**: from opening the page until a seeded pod name is on it.
  - **All pods listed**: until the page says it holds all of them (`10000 of 10000 pods`).
  - **Server memory**: the process's peak resident memory (`VmHWM`) by then.
  - **Page memory**: the JavaScript heap after the list is up and the garbage is collected.
- Kubri runs with its own empty settings and opens on pods (`defaultView: pods`).

## Running it yourself

You need a cluster you can fill (k3s, kind or minikube), `kubectl`, Node with Playwright, and a release build:

```sh
bench/speed/seed.sh 10000
(cd ui && pnpm build) && cargo build --release -p kubri-server
node bench/speed/run.cjs
```

`BENCH_PODS`, `BENCH_RUNS` and `BENCH_OUT` change the pod count, the runs and the result file. Remove the pods afterwards with `kubectl delete namespace bench`.

The limits are in [`bench/speed/limits.json`](../bench/speed/limits.json). They leave room for slower CI machines; a change that needs more should say why.
