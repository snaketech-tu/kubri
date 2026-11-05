## What's new

- **Kubri in German:** **Language** under Settings → Appearance picks English, German or the system's language. The words of Kubernetes, such as Pod and Deployment, stay English.
- **A Kubri for your team:** kubri-server signs people in with your OpenID Connect provider, each person works with their own Kubernetes permissions, and every change goes to an audit log. A Helm chart installs it; see [A Kubri for your team](https://github.com/snaketech-tu/kubri/blob/main/docs/TEAMS.md).
- **Feedback from the app:** `:feedback` and the links in the help open a GitHub issue with your version filled in.
- **Coming from k9s:** a [new page](https://github.com/snaketech-tu/kubri/blob/main/docs/FROM-K9S.md) covers the first few minutes.
- **Fix:** closing a shell in Kubri ends the shell in the container too.

## Install

**macOS:** download the `.dmg` for your Mac (`aarch64` for Apple silicon, `x64` for Intel), open it and drag Kubri into Applications. Kubri is not signed by Apple yet, so macOS asks before opening it the first time: open it once, then choose **Open Anyway** under System Settings → Privacy & Security.

**Windows:** run the `-setup.exe` or the `.msi`. If SmartScreen warns about an unknown app, choose **More info** → **Run anyway**.

**Linux:** install the `.deb` (Debian, Ubuntu) or the `.rpm` (Fedora, openSUSE), or run the `.AppImage` on any distribution after making it executable.

**Browser:** run the Docker image `ghcr.io/snaketech-tu/kubri`, as described in [Running Kubri in the browser](https://github.com/snaketech-tu/kubri/blob/main/docs/WEB.md).
