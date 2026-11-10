<script lang="ts">
	import { pluginCommand } from '../actions';
	import { targetName } from '../action-dialog';
	import type { Plugin, PluginRun } from '../api';
	import { t } from '../i18n';
	import { inputVariables } from '../k9s';
	import { plainOptions } from '../select';
	import Phrase from './Phrase.svelte';
	import Select from './Select.svelte';

	let {
		plugin,
		run,
		variables,
		inputs = $bindable()
	}: { plugin: Plugin; run: PluginRun; variables: Record<string, string>; inputs: Record<string, string> } = $props();

	const subject = $derived(run.target ?? run.release);
	/** The plugin's command as it will run, with the inputs as typed. */
	const command = $derived(pluginCommand(plugin, { ...variables, ...inputVariables(plugin, inputs) }, run));
</script>

<p>
	{#if subject}
		<Phrase text={t('Run the plugin {plugin} on {name}?')} strong={{ plugin: plugin.name, name: targetName(subject) }} />
	{:else}
		<Phrase text={t('Run the plugin {plugin}?')} strong={{ plugin: plugin.name }} />
	{/if}
</p>
{#each plugin.inputs as input (input.name)}
	{@const label = input.label || input.name}
	{#if input.type === 'bool'}
		<label class="row">
			<input
				type="checkbox"
				checked={inputs[input.name] === 'true'}
				onchange={(e) => (inputs[input.name] = String(e.currentTarget.checked))}
			/>
			{label}
		</label>
	{:else if input.type === 'dropdown'}
		<label class="field">
			{label}{input.required ? ' *' : ''}
			<Select
				bind:value={inputs[input.name]}
				options={[...(input.required ? [] : [{ value: '', label: t('(none)') }]), ...plainOptions(input.options)]}
			/>
		</label>
	{:else}
		<label class="field">
			{label}{input.required ? ' *' : ''}
			<input
				inputmode={input.type === 'number' ? 'decimal' : undefined}
				bind:value={inputs[input.name]}
				spellcheck="false"
			/>
		</label>
	{/if}
{/each}
<pre class="command">{command}</pre>
{#if plugin.dangerous}
	<p class="muted">{t('Its k9s configuration marks it as dangerous.')}</p>
{/if}

<style>
	p {
		margin: 0 0 10px;
	}
	.muted {
		color: var(--text-muted);
	}
	.row {
		display: flex;
		align-items: center;
		gap: 8px;
		margin: 8px 0;
	}
	.field {
		display: flex;
		flex-direction: column;
		gap: 4px;
		margin: 10px 0;
		font-size: 13px;
		color: var(--text-muted);
	}
	.command {
		margin: 0 0 12px;
		padding: 8px 10px;
		border-radius: 6px;
		background: var(--surface-2);
		font-family: var(--mono);
		font-size: 12.5px;
		white-space: pre-wrap;
		word-break: break-all;
	}
</style>
