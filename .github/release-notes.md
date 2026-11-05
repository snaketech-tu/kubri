## What's new

- **Try Kubri without a cluster:** without a kubeconfig, **Open the demo** opens a small shop that comes with the app, and **Start a cluster…** makes one on this computer with kind or k3d.
- **Why a container does not run:** for an image that cannot be pulled, a missing Secret, out of memory, a crash loop or a pod with no room, Kubri says what is wrong in one sentence and offers the action that helps.
- **A clearer layout:** a new sidebar with one name per view, lists that fit the window (**All columns** with <kbd>w</kbd>), an object's actions in one place, and a card with **Cancel** while a context connects.
- **Roll back StatefulSets and DaemonSets** to an earlier revision, like Deployments.
- **Command line:** `kubri lint` and `kubri snapshot` run the checks and save snapshots in CI pipelines, with SARIF for GitHub code scanning.
- **Web app:** the unused Prometheus query route is gone, so signed-in users can no longer run their own PromQL through the server.
- **Fixes:** drain checks that it may evict pods before it cordons the node; read-only says the real reason; status colors in light mode have more contrast; a snapshot opens without a kubeconfig.

## Install

**macOS:** download the `.dmg` for your Mac (`aarch64` for Apple silicon, `x64` for Intel), open it and drag Kubri into Applications. Kubri is not signed by Apple yet, so macOS asks before opening it the first time: open it once, then choose **Open Anyway** under System Settings → Privacy & Security.

**Windows:** run the `-setup.exe` or the `.msi`. If SmartScreen warns about an unknown app, choose **More info** → **Run anyway**.

**Linux:** install the `.deb` (Debian, Ubuntu) or the `.rpm` (Fedora, openSUSE), or run the `.AppImage` on any distribution after making it executable.

**Browser:** run the Docker image `ghcr.io/snaketech-tu/kubri`, as described in [Running Kubri in the browser](https://github.com/snaketech-tu/kubri/blob/main/docs/WEB.md).
