# Local terminal

Kubri can open a shell on your own computer in which `kubectl` and `helm` already point at the context and namespace Kubri shows. You do not run `kubectl config use-context` or pass `-n`: what you see in Kubri is what the commands in this terminal talk to.

## What it sets up

- **The shell.** The **Shell** setting when you set one; otherwise your login shell (`$SHELL -l`) on Linux and macOS, and PowerShell 7 (`pwsh`), or Windows PowerShell when that is missing, on Windows. To use WSL, set the shell to `wsl.exe`.
- **The folder.** The **Start folder** setting, or your home folder.
- **`KUBECONFIG`.** A small file of its own first, then the files kubectl would read anyway (`KUBECONFIG`, or `~/.kube/config`), then the contexts you added in Kubri. The small file holds only `current-context` and a copy of the context entry with the namespace Kubri shows. kubectl and helm merge the list with the first file winning, so the cluster and user entries, and with them every credential, still come from your own files. Nothing secret is written to disk.
- **`KUBRI_CONTEXT` and `KUBRI_NAMESPACE`**, for your prompt, and `TERM=xterm-256color`.

The file lives in Kubri's settings folder under `terminals/`, can only be read by you (mode `0600`), and is removed when the terminal closes. Files left behind by a Kubri that did not exit cleanly are removed after a day, the next time Kubri starts.

A terminal keeps the context and namespace it started with. Switching the context in Kubri does not change a shell that is already open; the next terminal follows the new context. `kubectl config use-context` inside the terminal only changes that terminal, as it writes to its own small file.

## kubectl and helm

Before the shell opens, Kubri looks for `kubectl` and `helm` on the `PATH` and asks them for their versions. The terminal's header shows them. When `kubectl` is missing, or its minor version is more than one away from the cluster's (outside the [supported version skew](https://kubernetes.io/releases/version-skew-policy/#kubectl)), the header says so. The shell opens either way.

## Guard rails

A shell can do anything, so Kubri only opens one where it would let you change the cluster itself:

- **Read-only** (`--read-only` or `KUBRI_READ_ONLY=true`) and **locked contexts** get no local terminal, and Kubri says why. Unlocking a locked context allows a terminal while the unlock lasts.
- **Production contexts** open with a red first line: "Production: commands here run against *context* without Kubri's checks."
- **AI agents** using Kubri over MCP cannot open a local terminal.
- **Snapshots** have no cluster to talk to, so they get no terminal.

## In the web app

In the web app, a "local" terminal runs on the server, as the user the server runs as. It is therefore off unless the server is started with `--local-terminal` or `KUBRI_LOCAL_TERMINAL=true`, and the server logs a warning when it is on. Turn it on only when everyone who can sign in may run commands on that machine, for example when you run the server on your own computer.

## Settings

See [Settings](SETTINGS.md#terminal): shell and its arguments, start folder, font size.
