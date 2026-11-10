# Contributing to Kubri

Thanks for wanting to help. Bug reports, ideas, docs and code are all welcome.

## Reporting a problem or an idea

- **A problem**: [report it](https://github.com/snaketech-tu/kubri/issues/new?template=bug.yml). From inside Kubri, `:feedback` or the link at the bottom of the help (<kbd>?</kbd>) fills in your version and system.
- **An idea**: [suggest it](https://github.com/snaketech-tu/kubri/issues/new?template=idea.yml).
- **A security problem**: report it privately, as [SECURITY.md](SECURITY.md) describes.

Please leave out secrets, tokens and cluster names you do not want to be public.

## Your first change

Issues labelled [good first issue](https://github.com/snaketech-tu/kubri/labels/good%20first%20issue) are small and well described. For anything larger, open an issue first and say how you would go about it, so nobody builds something that cannot be merged.

## How Kubri is built

One Rust core talks to the cluster. A Tauri desktop app and an axum web server both expose it to one Svelte UI.

| Path | What it is |
| --- | --- |
| `crates/kubri-core` | All cluster access, built on kube-rs |
| `crates/kubri-server` | Web mode: REST and WebSocket API, sign-in, audit log |
| `crates/kubri-mcp` | MCP server for AI agents |
| `crates/kubri-cli` | `lint` and `snapshot` commands |
| `ui/src-tauri` | Desktop app |
| `ui` | SvelteKit and TypeScript UI used by both apps |
| `docs/` | User documentation |

[AGENTS.md](AGENTS.md) describes the layout, the conventions and the checks in more detail. It is written for coding agents, but it is the most complete guide for people too.

## Setting up

You need stable Rust, Node.js 26 and pnpm 10. On Linux the desktop app also needs the [Tauri system dependencies](https://tauri.app/start/prerequisites/).

```sh
cd ui && pnpm install --frozen-lockfile

pnpm tauri dev                 # desktop app with hot reload
cargo run -p kubri-server      # from the repository root, then in ui/:
pnpm dev                       # browser on http://localhost:5173
```

## Checks

CI runs exactly these, so run them before you open a pull request:

```sh
cd ui && pnpm check && pnpm test && pnpm build
cd .. && cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Changes that only show against a real cluster, such as logs, shells, port-forwards and watches, are tested end to end against k3s in `scripts/e2e/`.

## What makes a change easy to merge

- **Small and focused.** One change per pull request, branched off `main`.
- **Both apps behave the same.** A new backend call goes through the core and is used by both the desktop app and the web server.
- **Safety lives in the core.** Changes to a cluster check read-only mode and permissions in `kubri-core`, never only in the UI.
- **k9s keys keep working.** Do not change what an existing k9s key, alias or setting does.
- **Plain words.** UI text, docs and error messages are short, plain English sentences. UI text goes through `t()` with its German translation in `ui/src/lib/i18n/de.ts`; `pnpm test` tells you when one is missing.
- **Tests.** Rust tests go in a `mod tests` at the bottom of the module, UI tests next to the module in `ui/src/lib/*.test.ts`.
- **Docs.** Update the matching page in `docs/` when something people see changes.
- **Style.** Rust follows `rustfmt`. The UI has no formatter: match the surrounding code (tabs, single quotes), and do not run Prettier over `ui/`.

Pull requests are squash-merged, so the title becomes the commit message. Describe the change in plain words, like "Show the logs of every container at once".

## License

By contributing you agree that your contribution is licensed under the [Apache License 2.0](LICENSE), like the rest of Kubri.

Everyone taking part follows the [code of conduct](CODE_OF_CONDUCT.md).
