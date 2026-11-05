## What's new

- **Requests and limits in one dialog:** <kbd>Shift</kbd>+<kbd>Q</kbd> sets CPU and memory for all containers of a workload, with the room left on the nodes and in quotas.
- **Limits in the pod list:** CPU and Mem show the limit next to the use, and the Nodes list shows the room left on each node.
- **Start where you left off:** Kubri opens the context and view you used last.
- **Port-forwards stay open**, and favorites start or stop with one click from the sidebar.
- **Production namespaces:** mark single namespaces as production for the same care as a production context.
- **Other label names for logs**, such as `exported_namespace`, saved as templates for every context.
- Pinned places come first in the sidebar, the Port-forwards page has bigger entries, and the Integrations page has a clearer layout.

## Install

**macOS:** download the `.dmg` for your Mac (`aarch64` for Apple silicon, `x64` for Intel), open it and drag Kubri into Applications. Kubri is not signed by Apple yet, so macOS asks before opening it the first time: open it once, then choose **Open Anyway** under System Settings → Privacy & Security.

**Windows:** run the `-setup.exe` or the `.msi`. If SmartScreen warns about an unknown app, choose **More info** → **Run anyway**.

**Linux:** install the `.deb` (Debian, Ubuntu) or the `.rpm` (Fedora, openSUSE), or run the `.AppImage` on any distribution after making it executable.

**Browser:** run the Docker image `ghcr.io/snaketech-tu/kubri`, as described in [Running Kubri in the browser](https://github.com/snaketech-tu/kubri/blob/main/docs/WEB.md).
