## What's new

Kubri 1.13 makes Explain work with the AI tools you already use, and port-forward favorites easier to keep in bulk.

- **Explain through Claude Code, Codex or OpenRouter.** Claude Code and Codex run with the account you signed in with, so no API key is needed. OpenRouter reaches many providers' models with one key, and each provider now keeps its own key.
- **Names for port-forward favorites**, shown in the sidebar instead of the target.
- **Many favorites at once:** from marked rows, every Service of a namespace, pasted `kubectl port-forward` commands or a favorites file, with free local ports picked for you. **Export favorites** saves them to share, and the sidebar groups many favorites by namespace.
- **Grafana behind a client certificate.** Kubri asks the Service behind the Ingress instead, and your browser still opens the URL.
- **Right-sizing finds workloads with a dash in their name**, like `order-api`, which were missing before.

The full list is in the [changelog](https://github.com/snaketech-tu/kubri/blob/main/CHANGELOG.md).

## Install

**macOS:** download the `.dmg` for your Mac (`aarch64` for Apple silicon, `x64` for Intel), open it and drag Kubri into Applications. Kubri is not signed by Apple yet, so macOS asks before opening it the first time: open it once, then choose **Open Anyway** under System Settings → Privacy & Security.

**Windows:** run the `-setup.exe` or the `.msi`. If SmartScreen warns about an unknown app, choose **More info** → **Run anyway**.

**Linux:** install the `.deb` (Debian, Ubuntu) or the `.rpm` (Fedora, openSUSE), or run the `.AppImage` on any distribution after making it executable.

**Browser:** run the Docker image `ghcr.io/snaketech-tu/kubri`, as described in [Running Kubri in the browser](https://github.com/snaketech-tu/kubri/blob/main/docs/WEB.md).
