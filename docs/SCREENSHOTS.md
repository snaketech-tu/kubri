# README screenshots

The images in `docs/assets/screenshots/` are screenshots of the built Svelte application. They use the selected **02 / Link** branding and synthetic Kubernetes data. They are not rendered from the design-study HTML and do not read a kubeconfig or contact a cluster.

The capture covers the pod list in light/dark mode, cluster overview, pod inspector, previous-container logs, topology and command palette. The clock and sample data are controlled for reproducible output. Object events are scoped to their selected pod.

## Capture

Build and serve the application from `ui/`:

```sh
pnpm install --frozen-lockfile
pnpm build
pnpm preview --host 127.0.0.1 --port 4190
```

From the repository root, with Playwright and its Chromium browser installed:

```sh
node scripts/capture-readme.cjs
```

The script also accepts an absolute path to a Playwright module, a browser executable and a preview URL:

```sh
node scripts/capture-readme.cjs /path/to/playwright /path/to/browser http://127.0.0.1:4190
```

All API requests and WebSockets are intercepted inside the isolated browser context. Screenshots are captured through the real application controls. The script rejects mutation attempts and fails on browser JavaScript errors.

Version labels come from `ui/package.json`; regenerate the screenshots after a version or branding change. Keep both `pods-light.png` and `pods-dark.png`, as the README chooses its main image using the reader's color preference.
