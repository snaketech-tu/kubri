<script lang="ts">
	import { t } from '../i18n';
	import { backend, type ExplainClis, type ExplainProvider, type ExplainSettings } from '../api';
	import { ANTHROPIC_MODELS, CLAUDE_CODE_MODELS, providerName, runsCommand } from '../explain';
	import { errorMessage } from '../format';
	import { coreText } from '../i18n/errors';
	import { app } from '../state.svelte';
	import Phrase from './Phrase.svelte';

	const explain = $derived(app.prefs.settings.explain);
	let hasKey = $state<boolean | null>(null);
	let key = $state('');
	let saving = $state(false);

	/** Where Claude Code and Codex are, or why they cannot be used here; null until known. */
	let clis = $state<ExplainClis | null>(null);
	let checking = $state(false);

	/** Where to read how to install each tool. */
	const INSTALL: Record<string, string> = {
		claude: 'https://code.claude.com/docs/en/setup',
		codex: 'https://developers.openai.com/codex/cli'
	};

	const PROVIDERS = $derived.by(() => {
		const list: [ExplainProvider, string][] = [
			['none', t('Off')],
			['anthropic', 'Anthropic'],
			['openai', t('OpenAI-compatible')]
		];
		// Only where they can run, or while one is picked, so that the reason shows.
		for (const provider of ['claude', 'codex'] as const) {
			if ((clis && !clis.reason) || explain.provider === provider) list.push([provider, providerName(provider)]);
		}
		return list;
	});

	const cli = $derived(clis?.clis.find((c) => c.provider === explain.provider) ?? null);

	$effect(() => {
		backend.explainKey().then(
			(k) => (hasKey = k.hasKey),
			() => (hasKey = null)
		);
		check();
	});

	async function check() {
		checking = true;
		try {
			clis = await backend.explainClis();
		} catch {
			clis = null;
		} finally {
			checking = false;
		}
	}

	function update(change: Partial<ExplainSettings>) {
		app.prefs.setExplainSettings({ ...explain, ...change });
	}

	async function saveKey(value: string | null) {
		saving = true;
		try {
			hasKey = (await backend.setExplainKey(value)).hasKey;
			key = '';
			app.toast(value ? t('Saved the API key') : t('Removed the API key'));
		} catch (err) {
			app.toast(errorMessage(err), 'bad');
		} finally {
			saving = false;
		}
	}

	/** Whether the key goes into the system keychain, or else into a file only the user can read. */
	const keychain = $derived(backend.desktop && /Mac|Win/.test(navigator.userAgent));
</script>

<div class="block">
	<h2 id="config-explain">{t('Explain')}</h2>
	<p class="muted">
		{t('Explain asks a model of your choice about a pod, an event, log lines, a finding or a failed action, with your own API key or through Claude Code or Codex if you use them. You always see what will be sent first, with secret values hidden. While it is off, nothing is sent anywhere.')}
	</p>

	<div class="providers" role="radiogroup" aria-label={t('Provider')}>
		{#each PROVIDERS as [value, label] (value)}
			<label class:active={explain.provider === value}>
				<input
					type="radio"
					name="explain-provider"
					{value}
					checked={explain.provider === value}
					onchange={() => update({ provider: value, model: '', baseUrl: '' })}
				/>
				{label}
			</label>
		{/each}
	</div>

	{#if runsCommand(explain.provider)}
		{@const command = explain.provider}
		<div class="fields">
			{#if clis?.reason}
				<p class="state" role="status">{coreText(clis.reason)}</p>
			{:else if clis && !cli?.path}
				<p class="state" role="status">
					<Phrase text={t('Kubri finds no {command} command on this computer. Install {name}, sign in once, then check again.')} code={{ command }} strong={{ name: providerName(command) }} />
					<a href={INSTALL[command]} target="_blank" rel="noreferrer">{t('How to install {name}', { name: providerName(command) })}</a>
				</p>
			{:else if cli?.path}
				<p class="muted small found">
					<Phrase
						text={cli.version ? t('Runs {path}, version {version}, with the account you signed in with there. It needs no API key.') : t('Runs {path} with the account you signed in with there. It needs no API key.')}
						code={{ path: cli.path, version: cli.version ?? '' }}
					/>
				</p>
			{/if}
			<label>
				<span>{t('Model')}</span>
				<input
					list="explain-cli-models"
					value={explain.model}
					placeholder={t('{name}’s own default', { name: providerName(command) })}
					onchange={(e) => update({ model: e.currentTarget.value })}
				/>
				{#if command === 'claude'}
					<datalist id="explain-cli-models">
						{#each CLAUDE_CODE_MODELS as model (model)}<option value={model}></option>{/each}
					</datalist>
				{/if}
			</label>
			<p class="muted small hint">
				{command === 'claude'
					? t('Kubri runs it in an empty folder with all its tools turned off, so it only sees what you saw before sending.')
					: t('Kubri runs it in an empty folder in its read-only sandbox, so it cannot change anything.')}
				{#if !clis?.reason}
					<button type="button" class="link" disabled={checking} onclick={check}>{t('Check again')}</button>
				{/if}
			</p>
		</div>
	{:else if explain.provider !== 'none'}
		<div class="fields">
			<label>
				<span>{t('Model')}</span>
				<input
					list="explain-models"
					value={explain.model}
					placeholder={explain.provider === 'anthropic' ? ANTHROPIC_MODELS[0] : t('such as gpt-5 or llama3.2')}
					onchange={(e) => update({ model: e.currentTarget.value })}
				/>
				{#if explain.provider === 'anthropic'}
					<datalist id="explain-models">
						{#each ANTHROPIC_MODELS as model (model)}<option value={model}></option>{/each}
					</datalist>
				{/if}
			</label>
			<label>
				<span>{t('Address')}</span>
				<input
					value={explain.baseUrl}
					placeholder={explain.provider === 'anthropic' ? 'https://api.anthropic.com' : 'https://api.openai.com/v1'}
					onchange={(e) => update({ baseUrl: e.currentTarget.value })}
				/>
			</label>
			{#if explain.provider === 'openai'}
				<p class="muted small hint">
					<Phrase
						text={t('Any endpoint with OpenAI’s chat completions works. For Ollama on this computer, use {url} and a model you pulled; it needs no key.')}
						code={{ url: 'http://localhost:11434/v1' }}
					/>
				</p>
			{/if}
			<form
				class="key"
				onsubmit={(e) => {
					e.preventDefault();
					if (key.trim()) saveKey(key);
				}}
			>
				<span>{t('API key')}</span>
				<input
					type="password"
					autocomplete="off"
					bind:value={key}
					placeholder={hasKey ? t('A key is stored; type a new one to replace it') : t('Paste your key')}
					aria-label={t('API key')}
				/>
				<button type="submit" class="small" disabled={saving || !key.trim()}>{t('Save')}</button>
				{#if hasKey}<button type="button" class="small" disabled={saving} onclick={() => saveKey(null)}>{t('Remove')}</button>{/if}
			</form>
			<p class="muted small hint">
				{#if hasKey}{keychain ? t('A key is stored in the system keychain.') : t('A key is stored in a file in Kubri’s config folder that only you can read.')}{:else}{t('No key is stored.')}{/if} {t('Kubri never shows it again and never writes it into the settings file.')}
			</p>
		</div>
	{/if}
</div>

<style>
	.block {
		margin-bottom: 14px;
		padding: 16px 18px;
		border: 1px solid var(--border);
		border-radius: 10px;
		background: var(--surface-1);
	}
	h2 {
		margin: 0 0 8px;
		font-size: 15px;
	}
	p {
		margin: 0 0 12px;
	}
	.muted {
		color: var(--text-muted);
	}
	.small {
		font-size: 12px;
	}
	/* Holds the hidden radio buttons, which would otherwise stretch the window. */
	.providers {
		position: relative;
		display: inline-flex;
		flex-wrap: wrap;
		max-width: 100%;
		margin-bottom: 14px;
		border: 1px solid var(--border);
		border-radius: 8px;
		overflow: hidden;
	}
	.providers label {
		padding: 6px 12px;
		font-size: 13px;
		cursor: pointer;
	}
	.providers label + label {
		border-left: 1px solid var(--border);
	}
	.providers label.active {
		background: var(--accent);
		color: var(--on-accent);
	}
	.providers label:focus-within {
		outline: 2px solid var(--accent);
		outline-offset: -2px;
	}
	.providers input {
		position: absolute;
		opacity: 0;
		pointer-events: none;
	}
	.fields {
		display: grid;
		gap: 10px;
		max-width: 620px;
	}
	.fields > label,
	.key {
		display: grid;
		grid-template-columns: 80px 1fr;
		align-items: center;
		gap: 10px;
		font-size: 13px;
	}
	.key {
		grid-template-columns: 80px 1fr auto auto;
		margin: 0;
	}
	.hint {
		margin: -4px 0 0 90px;
	}
	.state,
	.found {
		margin: 0;
	}
	.state {
		padding: 8px 10px;
		border: 1px solid var(--border);
		border-radius: 6px;
		background: var(--surface-2);
		font-size: 13px;
	}
	.state a {
		margin-left: 6px;
	}
	.found :global(code),
	.state :global(code) {
		font-family: var(--mono);
		word-break: break-all;
	}
	button.link {
		padding: 0;
		border: 0;
		background: none;
		color: var(--accent);
		font: inherit;
		cursor: pointer;
	}
	button.link:disabled {
		color: var(--text-muted);
		cursor: default;
	}
	input:not([type='radio']) {
		min-width: 0;
		padding: 6px 8px;
		border: 1px solid var(--border);
		border-radius: 6px;
		background: var(--surface-1);
		color: var(--text);
		font-size: 13px;
	}
	.hint :global(code) {
		font-family: var(--mono);
	}
	button.small {
		padding: 5px 10px;
		border: 1px solid var(--border);
		border-radius: 6px;
		background: var(--surface-1);
		font-size: 12px;
	}
	@media (max-width: 640px) {
		.fields > label,
		.key {
			grid-template-columns: 1fr;
		}
		.hint {
			margin-left: 0;
		}
	}
</style>
