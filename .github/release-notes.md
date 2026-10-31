## What's new

Kubri v1.4.0 brings the Link identity and a refreshed workspace for desktop and browser.

- New blue Link logo, vector wordmarks, favicon and native application icons.
- Clearer cluster context, grouped navigation and layouts that adapt to smaller windows.
- An overview that prioritizes warnings, plus a pod inspector with direct access to previous-container logs.
- Shared protection for unsaved YAML, explicit cleanup targets, and context-bound object actions.
- Stable metric columns with clearer stale, unavailable, permission and connection states.
- Updated documentation screenshots captured from the application with synthetic demo data.

## Install

**macOS:** download the `.dmg` for your Mac (`aarch64` for Apple silicon, `x64` for Intel), open it and drag Kubri into Applications. Kubri is not signed by Apple yet, so macOS asks before opening it the first time: open it once, then choose **Open Anyway** under System Settings → Privacy & Security.

**Windows:** run the `-setup.exe` or the `.msi`. If SmartScreen warns about an unknown app, choose **More info** → **Run anyway**.

**Linux:** install the `.deb` (Debian, Ubuntu) or the `.rpm` (Fedora, openSUSE), or run the `.AppImage` on any distribution after making it executable.

**Browser:** run the Docker image `ghcr.io/snaketech-tu/kubri`, as described in [Running Kubri in the browser](https://github.com/snaketech-tu/kubri/blob/main/docs/WEB.md).
