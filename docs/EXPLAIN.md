# Explain

**Explain** asks a model of your choice what a pod, an event, some log lines, a finding of Checks or a failed action means, and shows the answer beside it. It uses your own API key, or Claude Code or Codex if you already use one of them. Until you pick a provider in **Settings → Explain** (or `:explain`), Kubri sends nothing anywhere.

## Setting it up

| Provider | What to enter |
| --- | --- |
| Anthropic | Your Anthropic API key. The model defaults to `claude-opus-5-5`; `claude-sonnet-5-5` and `claude-haiku-5-5` answer faster and cost less. |
| OpenAI-compatible | The address and model of any endpoint with OpenAI's chat completions (`…/chat/completions`): OpenAI, a gateway, or a local model. For [Ollama](https://ollama.com), use `http://localhost:11434/v1` and a model you pulled, such as `llama3.2`; it needs no key. |
| OpenRouter | Your [OpenRouter](https://openrouter.ai) key. One key reaches the models of many providers; name one as [openrouter.ai/models](https://openrouter.ai/models) lists it, or leave the model empty and OpenRouter's router, `openrouter/auto`, picks one for each question. The answer says which model wrote it. |
| Claude Code | Nothing, if [Claude Code](https://code.claude.com/docs/en/setup) is installed and you signed in by running `claude` once. Kubri runs `claude -p` with your account. The model is optional: a name like `sonnet` or `claude-haiku-5-5`, or empty for Claude Code's own choice. |
| Codex | Nothing, if [Codex](https://developers.openai.com/codex/cli) is installed and you signed in with `codex login`. Kubri runs `codex exec` with your account. The model is optional. |

The address can also point Anthropic requests at a proxy or gateway.

Kubri looks for `claude` and `codex` on your `PATH`, and also where their installers and npm put them, as an app started from a launcher often has a short `PATH`. The settings show the program Kubri found and its version; **Check again** looks once more after you installed one. Kubri runs it in an empty folder, with the prompt you saw on its input. Claude Code gets no tools and no MCP servers, so it only answers. Codex runs in its read-only sandbox: it may read files to answer, but cannot change anything. Neither keeps the session.

In the [web app](WEB.md), Claude Code and Codex would run on the server, with the account signed in there. So they are offered only when the server is started with `KUBRI_LOCAL_TERMINAL=true`, and never on a Kubri shared by a team.

Each provider has its own key, so a key only ever goes to the provider you saved it for. A key never goes into the settings file and is never shown again. The desktop app keeps it in the system keychain on macOS and Windows. On Linux, and for the web server, it is kept in `explain-key-anthropic`, `explain-key-openai` or `explain-key-openrouter` in Kubri's config folder, readable only by you. **Remove** deletes it. A key saved by an older version, which kept one key for all providers, is given to the provider in the settings.

## Using it

| Where | How |
| --- | --- |
| Any object, such as a crash-looping pod, an Event or a Deployment | **Explain…** in the action bar or row menu, or <kbd>Shift</kbd>+<kbd>E</kbd>. On pods it sits in the action bar once Explain is set up. |
| Logs | Select lines and press **Explain** (or <kbd>x</kbd>). Without a selection, the last 200 shown lines go. |
| Checks | <kbd>Shift</kbd>+<kbd>E</kbd> on a finding, or **Explain these findings…** in its menu. |
| A failed action | **Explain…** beside the error in the action's dialog. |

Kubri gathers what it knows first: the object's YAML, for a pod its likely problem, warning events and the last log lines of failing containers, for other objects their events, and for an Event the object it is about. The panel shows **exactly what will be sent**, to which provider, model and address, and nothing leaves Kubri before you press **Send**. The answer appears in the same panel, with **Copy answer** and **Ask again**.

## What is hidden

Before anything is shown or sent, Kubri removes:

- the values of Secrets (their keys stay), and the copy `kubectl apply` keeps in an annotation;
- values of environment variables whose names sound secret, such as `DB_PASSWORD` or `API_TOKEN`;
- in all text, logs included: private keys, JSON web tokens such as service account tokens, credentials after `Bearer` or `Basic`, passwords in URLs, keys with a known shape (Anthropic, OpenAI, GitHub, Slack, AWS, Google), and values after names such as `password`, `secret`, `token` or `api_key`.

They show as `<hidden>`. This catches the usual cases, not every possible one, which is why you see the prompt before it goes. Models can be wrong: check an answer before you act on it.
