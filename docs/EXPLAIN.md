# Explain

**Explain** asks a model of your choice what a pod, an event, some log lines, a sanitizer finding or a failed action means, and shows the answer beside it. It uses your own API key. Until you pick a provider in **Settings → Explain** (or `:explain`), Kubri sends nothing anywhere.

## Setting it up

| Provider | What to enter |
| --- | --- |
| Anthropic | Your Anthropic API key. The model defaults to `claude-opus-5-5`; `claude-sonnet-5-5` and `claude-haiku-5-5` answer faster and cost less. |
| OpenAI-compatible | The address and model of any endpoint with OpenAI's chat completions (`…/chat/completions`): OpenAI, a gateway, or a local model. For [Ollama](https://ollama.com), use `http://localhost:11434/v1` and a model you pulled, such as `llama3.2`; it needs no key. |

The address can also point Anthropic requests at a proxy or gateway.

The key never goes into the settings file and is never shown again. The desktop app keeps it in the system keychain on macOS and Windows. On Linux, and for the web server, it is kept in `explain-key` in Kubri's config folder, readable only by you. **Remove** deletes it.

## Using it

| Where | How |
| --- | --- |
| Any object, such as a crash-looping pod, an Event or a Deployment | **Explain…** in the action bar or row menu, or <kbd>Shift</kbd>+<kbd>E</kbd>. On pods it sits in the action bar once Explain is set up. |
| Logs | Select lines and press **Explain** (or <kbd>x</kbd>). Without a selection, the last 200 shown lines go. |
| Sanitizer | <kbd>Shift</kbd>+<kbd>E</kbd> on a finding, or **Explain these findings…** in its menu. |
| A failed action | **Explain…** beside the error in the action's dialog. |

Kubri gathers what it knows first: the object's YAML, for a pod its likely problem, warning events and the last log lines of failing containers, for other objects their events, and for an Event the object it is about. The panel shows **exactly what will be sent**, to which provider, model and address, and nothing leaves Kubri before you press **Send**. The answer appears in the same panel, with **Copy answer** and **Ask again**.

## What is hidden

Before anything is shown or sent, Kubri removes:

- the values of Secrets (their keys stay), and the copy `kubectl apply` keeps in an annotation;
- values of environment variables whose names sound secret, such as `DB_PASSWORD` or `API_TOKEN`;
- in all text, logs included: private keys, JSON web tokens such as service account tokens, credentials after `Bearer` or `Basic`, passwords in URLs, keys with a known shape (Anthropic, OpenAI, GitHub, Slack, AWS, Google), and values after names such as `password`, `secret`, `token` or `api_key`.

They show as `<hidden>`. This catches the usual cases, not every possible one, which is why you see the prompt before it goes. Models can be wrong: check an answer before you act on it.
