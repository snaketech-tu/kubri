<script lang="ts">
	import { backend, type ExplainProvider, type ExplainSettings } from '../api';
	import { ANTHROPIC_MODELS } from '../explain';
	import { errorMessage } from '../format';
	import { app } from '../state.svelte';

	const explain = $derived(app.settings.explain);
	let hasKey = $state<boolean | null>(null);
	let key = $state('');
	let saving = $state(false);

	const PROVIDERS: [ExplainProvider, string][] = [
		['none', 'Off'],
		['anthropic', 'Anthropic'],
		['openai', 'OpenAI-compatible']
	];

	$effect(() => {
		backend.explainKey().then(
			(k) => (hasKey = k.hasKey),
			() => (hasKey = null)
		);
	});

	function update(change: Partial<ExplainSettings>) {
		app.setExplainSettings({ ...explain, ...change });
	}

	async function saveKey(value: string | null) {
		saving = true;
		try {
			hasKey = (await backend.setExplainKey(value)).hasKey;
			key = '';
			app.toast(value ? 'Saved the API key' : 'Removed the API key');
		} catch (err) {
			app.toast(errorMessage(err), 'bad');
		} finally {
			saving = false;
		}
	}

	const where = $derived(
		backend.desktop && /Mac|Win/.test(navigator.userAgent)
			? 'in the system keychain'
			: 'in a file in Kubri’s config folder that only you can read'
	);
</script>

<div class="block">
	<h2 id="config-explain">Explain</h2>
	<p class="muted">
		Explain asks a model of your choice about a pod, an event, log lines, a finding or a failed action, with your own API
		key. You always see what will be sent first, with secret values hidden. While it is off, nothing is sent anywhere.
	</p>

	<div class="providers" role="radiogroup" aria-label="Provider">
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

	{#if explain.provider !== 'none'}
		<div class="fields">
			<label>
				<span>Model</span>
				<input
					list="explain-models"
					value={explain.model}
					placeholder={explain.provider === 'anthropic' ? ANTHROPIC_MODELS[0] : 'such as gpt-5 or llama3.2'}
					onchange={(e) => update({ model: e.currentTarget.value })}
				/>
				{#if explain.provider === 'anthropic'}
					<datalist id="explain-models">
						{#each ANTHROPIC_MODELS as model (model)}<option value={model}></option>{/each}
					</datalist>
				{/if}
			</label>
			<label>
				<span>Address</span>
				<input
					value={explain.baseUrl}
					placeholder={explain.provider === 'anthropic' ? 'https://api.anthropic.com' : 'https://api.openai.com/v1'}
					onchange={(e) => update({ baseUrl: e.currentTarget.value })}
				/>
			</label>
			{#if explain.provider === 'openai'}
				<p class="muted small hint">
					Any endpoint with OpenAI’s chat completions works. For Ollama on this computer, use
					<code>http://localhost:11434/v1</code> and a model you pulled; it needs no key.
				</p>
			{/if}
			<form
				class="key"
				onsubmit={(e) => {
					e.preventDefault();
					if (key.trim()) saveKey(key);
				}}
			>
				<span>API key</span>
				<input
					type="password"
					autocomplete="off"
					bind:value={key}
					placeholder={hasKey ? 'A key is stored; type a new one to replace it' : 'Paste your key'}
					aria-label="API key"
				/>
				<button type="submit" class="small" disabled={saving || !key.trim()}>Save</button>
				{#if hasKey}<button type="button" class="small" disabled={saving} onclick={() => saveKey(null)}>Remove</button>{/if}
			</form>
			<p class="muted small hint">
				{#if hasKey}A key is stored {where}.{:else}No key is stored.{/if} Kubri never shows it again and never writes it into
				the settings file.
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
	.providers {
		display: inline-flex;
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
	input:not([type='radio']) {
		min-width: 0;
		padding: 6px 8px;
		border: 1px solid var(--border);
		border-radius: 6px;
		background: var(--surface-1);
		color: var(--text);
		font-size: 13px;
	}
	code {
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
