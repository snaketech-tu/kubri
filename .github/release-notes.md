## What's new

Kubri 1.12 is for people who work with the keyboard, and it asks your clusters less often.

- **Arrow keys like tabs.** Tab moves between the sidebar, the tabs, the toolbars and the list; the arrow keys move inside them. Shift+F10 opens the menu of a row.
- **A list opens with its first row chosen**, so Enter opens it right away.
- **The inspector without the mouse.** Tab goes into it, the arrow keys change its tab, Esc goes back to the list.
- **Backspace goes back**, and the breadcrumbs above each page can be clicked.
- **The command palette completes** namespaces and contexts: `:po sh` opens the pods of `shop`, Enter takes the first suggestion.
- **The YAML editor** has find and replace, go to line, Ctrl+D to delete a line and several cursors.
- **Gentler on your clusters.** Pages ask less often, not at all while Kubri is in the background, and the Clusters page no longer asks every cluster again and again.
- **Start a port-forward from the Port-forwards page**, choosing the namespace and the target in one dialog.

The full list is in the [changelog](https://github.com/snaketech-tu/kubri/blob/main/CHANGELOG.md).

## Install

**macOS:** download the `.dmg` for your Mac (`aarch64` for Apple silicon, `x64` for Intel), open it and drag Kubri into Applications. Kubri is not signed by Apple yet, so macOS asks before opening it the first time: open it once, then choose **Open Anyway** under System Settings → Privacy & Security.

**Windows:** run the `-setup.exe` or the `.msi`. If SmartScreen warns about an unknown app, choose **More info** → **Run anyway**.

**Linux:** install the `.deb` (Debian, Ubuntu) or the `.rpm` (Fedora, openSUSE), or run the `.AppImage` on any distribution after making it executable.

**Browser:** run the Docker image `ghcr.io/snaketech-tu/kubri`, as described in [Running Kubri in the browser](https://github.com/snaketech-tu/kubri/blob/main/docs/WEB.md).
