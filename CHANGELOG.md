# Changelog

## 1.5.0 — 2026-11-02

- Switching back to a context you just left is instant: up to three recent contexts (Settings, 0 to 5) stay connected for 30 minutes, and Kubri returns to the view, filter, cursor and namespace you left there. `:ctx -` and the top entry of the context switcher go to the previous context; a green dot marks contexts that are still connected. See [Several clusters](docs/CLUSTERS.md#switching-back-is-instant).
- Logs and shells of other contexts keep running in the workspace, with the context named on their tab.
- A local terminal with `kubectl` and `helm` set to the context and namespace Kubri shows: <kbd>Ctrl</kbd>+<kbd>`</kbd>, `:term` or the terminal button. No credentials are written; read-only and locked contexts get none; production contexts get a warning line. In the web app it is off unless the server is started with `KUBRI_LOCAL_TERMINAL=true`. See [Local terminal](docs/TERMINAL.md).
- Load kubeconfig files from anywhere in the app; Kubri keeps reading them where they are. Without a usable kubeconfig, a start page explains what to do. Dialogs close on a click outside.
- The README's Docker command works as written, and the docs show how to use many kubeconfig files spread over `~/.kube`, also in Docker.

## 1.4.1 — 2026-10-31

- Refine the Link logo to match the selected design, including its slanted stem and curved cutout.
- Refresh native icons, favicon and branding assets.
- Show the application version in the sidebar and add reproducible README screenshot capture.
- Stabilize the benchmark cancellation test on Windows.

## 1.4.0

See the [v1.4.0 release](https://github.com/snaketech-tu/kubri/releases/tag/v1.4.0).
