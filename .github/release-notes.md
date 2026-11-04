## What's new

- **Security fix for the web app:** contexts added with **Add clusters…** can no longer run commands or read files on the server. Before, anyone who could sign in could use them to run any command on the server or to send its files elsewhere. Such contexts now need the server to be started with `KUBRI_LOCAL_TERMINAL=true`. If you run Kubri in the browser for others, please update. **From URL** also no longer reads cloud metadata addresses.
- **Web app:** a reload keeps your view, addresses can be bookmarked, and the browser's Back button goes back inside Kubri.
- **Desktop app:** links to other sites, such as the install guides on the Integrations page, open in the browser.
- **Fixes:** lists over several namespaces no longer bring back deleted objects; web lists say why they cannot start; the Clusters page no longer signs in to every cluster each minute; **Add clusters…** reads a pasted kubeconfig right away; kube-prometheus-stack's Grafana is no longer taken for Prometheus.

## Install

**macOS:** download the `.dmg` for your Mac (`aarch64` for Apple silicon, `x64` for Intel), open it and drag Kubri into Applications. Kubri is not signed by Apple yet, so macOS asks before opening it the first time: open it once, then choose **Open Anyway** under System Settings → Privacy & Security.

**Windows:** run the `-setup.exe` or the `.msi`. If SmartScreen warns about an unknown app, choose **More info** → **Run anyway**.

**Linux:** install the `.deb` (Debian, Ubuntu) or the `.rpm` (Fedora, openSUSE), or run the `.AppImage` on any distribution after making it executable.

**Browser:** run the Docker image `ghcr.io/snaketech-tu/kubri`, as described in [Running Kubri in the browser](https://github.com/snaketech-tu/kubri/blob/main/docs/WEB.md).
