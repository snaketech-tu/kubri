## What's new

- **Grafana from any object:** paste a dashboard address once, then open or copy it for any pod, namespace or workload with **Grafana…**.
- **Fuller cluster overview:** health line, version, Ready workloads, and requested CPU and memory.
- **Terminal fonts:** prompts with Nerd Font icons show correctly everywhere; pick your own font in Settings.
- **Docker:** mounting `~/.kube` is enough, Kubri finds every kubeconfig in it.
- **Fixes:** read-only keys explain themselves, smoother YAML editing when others change the object, Enter on buttons reached with Tab, easier bulk deletes, an editable Scale field, and more accurate overview numbers.

## Install

**macOS:** download the `.dmg` for your Mac (`aarch64` for Apple silicon, `x64` for Intel), open it and drag Kubri into Applications. Kubri is not signed by Apple yet, so macOS asks before opening it the first time: open it once, then choose **Open Anyway** under System Settings → Privacy & Security.

**Windows:** run the `-setup.exe` or the `.msi`. If SmartScreen warns about an unknown app, choose **More info** → **Run anyway**.

**Linux:** install the `.deb` (Debian, Ubuntu) or the `.rpm` (Fedora, openSUSE), or run the `.AppImage` on any distribution after making it executable.

**Browser:** run the Docker image `ghcr.io/snaketech-tu/kubri`, as described in [Running Kubri in the browser](https://github.com/snaketech-tu/kubri/blob/main/docs/WEB.md).
