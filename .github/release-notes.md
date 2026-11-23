## What's new

Kubri 1.13.2 is a small update.

- **Copy a name from the inspector** with the new button right of the name.
- **Copying works over plain HTTP** in the browser, for names, kubectl commands and log lines.
- **CPU and memory line up** with the other columns in the lists.
- **Updated dependencies**, including fixes for two security advisories.

The full list is in the [changelog](https://github.com/snaketech-tu/kubri/blob/main/CHANGELOG.md).

## Install

**macOS:** download the `.dmg` for your Mac (`aarch64` for Apple silicon, `x64` for Intel), open it and drag Kubri into Applications. Kubri is not signed by Apple yet, so macOS asks before opening it the first time: open it once, then choose **Open Anyway** under System Settings → Privacy & Security.

**Windows:** run the `-setup.exe` or the `.msi`. If SmartScreen warns about an unknown app, choose **More info** → **Run anyway**.

**Linux:** install the `.deb` (Debian, Ubuntu) or the `.rpm` (Fedora, openSUSE), or run the `.AppImage` on any distribution after making it executable.

**Browser:** run the Docker image `ghcr.io/snaketech-tu/kubri`, as described in [Running Kubri in the browser](https://github.com/snaketech-tu/kubri/blob/main/docs/WEB.md).
