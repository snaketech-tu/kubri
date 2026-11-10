## What's new

- **Windows on Arm:** a native installer for Windows arm64.
- **The Helm chart as a package:** `helm install kubri oci://ghcr.io/snaketech-tu/charts/kubri`, no clone needed.
- **Settings that survive upgrades:** `persistence.enabled=true` keeps Kubri's settings on a volume.
- **Builds you can check:** attestations for the installers and the image, and an SBOM for the image.
- **Running a team Kubri safely:** a new section of the [team guide](https://github.com/snaketech-tu/kubri/blob/main/docs/TEAMS.md#running-it-safely).
- The desktop app and the web server now talk to the core in the same way. The web server's internal API changed; nothing outside Kubri used it.

## Install

**macOS:** download the `.dmg` for your Mac (`aarch64` for Apple silicon, `x64` for Intel), open it and drag Kubri into Applications. Kubri is not signed by Apple yet, so macOS asks before opening it the first time: open it once, then choose **Open Anyway** under System Settings → Privacy & Security.

**Windows:** run the `-setup.exe` or the `.msi`. If SmartScreen warns about an unknown app, choose **More info** → **Run anyway**.

**Linux:** install the `.deb` (Debian, Ubuntu) or the `.rpm` (Fedora, openSUSE), or run the `.AppImage` on any distribution after making it executable.

**Browser:** run the Docker image `ghcr.io/snaketech-tu/kubri`, as described in [Running Kubri in the browser](https://github.com/snaketech-tu/kubri/blob/main/docs/WEB.md).
