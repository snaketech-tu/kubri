## What's new

- **An overview that answers "is there room?":** CPU and memory show what running pods reserve, which decides whether a new pod fits, and name a pod that cannot start.
- **Warnings by workload:** Needs attention has one entry per Deployment or CronJob, in plain words, with the last line a crashing container logged.
- **Amounts you can read:** CPU in cores and memory in MiB, with kubectl's units as a setting. Throttled pods say so, and a pod's inspector shows its use right now.
- **Namespaces at a glance:** healthy pods, use and requests per namespace, and production namespaces marked with one click or by rules.

## Install

**macOS:** download the `.dmg` for your Mac (`aarch64` for Apple silicon, `x64` for Intel), open it and drag Kubri into Applications. Kubri is not signed by Apple yet, so macOS asks before opening it the first time: open it once, then choose **Open Anyway** under System Settings → Privacy & Security.

**Windows:** run the `-setup.exe` or the `.msi`. If SmartScreen warns about an unknown app, choose **More info** → **Run anyway**.

**Linux:** install the `.deb` (Debian, Ubuntu) or the `.rpm` (Fedora, openSUSE), or run the `.AppImage` on any distribution after making it executable.

**Browser:** run the Docker image `ghcr.io/snaketech-tu/kubri`, as described in [Running Kubri in the browser](https://github.com/snaketech-tu/kubri/blob/main/docs/WEB.md).
