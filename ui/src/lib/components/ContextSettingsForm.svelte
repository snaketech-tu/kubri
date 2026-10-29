<script lang="ts">
	import type { ContextSettings } from '../api';
	import { app } from '../state.svelte';

	let { context, namespaces = [] }: { context: string; namespaces?: string[] } = $props();

	/** Accent colors that read well on both light and dark backgrounds. */
	const COLORS: [string, string][] = [
		['#d64545', 'Red'],
		['#e07b24', 'Orange'],
		['#c9a227', 'Yellow'],
		['#1f9d55', 'Green'],
		['#0f9b8e', 'Teal'],
		['#326ce5', 'Blue'],
		['#7c5cff', 'Violet'],
		['#d63f8c', 'Pink'],
		['#6b7280', 'Gray']
	];

	const current = $derived(app.contextSettingsOf(context));
	let custom = $state('');

	function save(change: Partial<ContextSettings>) {
		app.setContextSettings(context, { ...current, ...change });
	}

	function text(field: 'displayName' | 'label' | 'namespace') {
		return (e: Event & { currentTarget: HTMLInputElement | HTMLSelectElement }) => {
			const value = e.currentTarget.value.trim();
			if (value !== current[field]) save({ [field]: value });
		};
	}

	function setProduction(production: boolean) {
		// A production context starts locked and red unless the user already chose otherwise.
		const change: Partial<ContextSettings> = { production };
		if (production && !current.production) {
			change.locked = true;
			if (!current.color) change.color = '#d64545';
			if (!current.label) change.label = 'Production';
		}
		save(change);
	}
</script>

<div class="form">
	<label class="field">
		<span>Display name</span>
		<input value={current.displayName} placeholder={context} onchange={text('displayName')} spellcheck="false" />
	</label>
	<label class="field">
		<span>Label</span>
		<input value={current.label} placeholder="e.g. Staging" onchange={text('label')} />
	</label>
	<div class="field">
		<span id="color-{context}">Color</span>
		<div class="colors" role="radiogroup" aria-labelledby="color-{context}">
			<button
				class="swatch none"
				role="radio"
				aria-checked={!current.color}
				title="No color"
				aria-label="No color"
				onclick={() => save({ color: '' })}
			></button>
			{#each COLORS as [hex, name] (hex)}
				<button
					class="swatch"
					role="radio"
					aria-checked={current.color === hex}
					title={name}
					aria-label={name}
					style:--swatch={hex}
					onclick={() => save({ color: hex })}
				></button>
			{/each}
			<input
				class="custom"
				aria-label="Custom color"
				placeholder="#rrggbb"
				value={COLORS.some(([hex]) => hex === current.color) ? custom : current.color}
				oninput={(e) => (custom = e.currentTarget.value)}
				onchange={(e) => {
					const value = e.currentTarget.value.trim();
					if (/^#[0-9a-f]{6}$/i.test(value)) save({ color: value.toLowerCase() });
					else if (value) app.toast('Colors look like #326ce5', 'bad');
				}}
				spellcheck="false"
			/>
		</div>
	</div>
	<label class="field">
		<span>Namespace after connecting</span>
		{#if namespaces.length}
			<select value={current.namespace} onchange={text('namespace')}>
				<option value="">All namespaces</option>
				{#each namespaces as ns (ns)}<option value={ns}>{ns}</option>{/each}
				{#if current.namespace && !namespaces.includes(current.namespace)}
					<option value={current.namespace}>{current.namespace}</option>
				{/if}
			</select>
		{:else}
			<input value={current.namespace} placeholder="All namespaces" onchange={text('namespace')} spellcheck="false" />
		{/if}
	</label>
	<label class="check">
		<input type="checkbox" checked={current.production} onchange={(e) => setProduction(e.currentTarget.checked)} />
		<span>
			<strong>Production</strong>
			<small>A banner shows it, and deleting asks you to type the name.</small>
		</span>
	</label>
	<label class="check">
		<input type="checkbox" checked={current.locked} onchange={(e) => save({ locked: e.currentTarget.checked })} />
		<span>
			<strong>Start locked</strong>
			<small>Read-only until you unlock it, for 15 minutes at a time.</small>
		</span>
	</label>
	<label class="check">
		<input type="checkbox" checked={current.readOnly} onchange={(e) => save({ readOnly: e.currentTarget.checked })} />
		<span>
			<strong>Read-only</strong>
			<small>Kubri never changes this cluster until you turn this off.</small>
		</span>
	</label>
	<label class="check">
		<input
			type="checkbox"
			checked={current.agentWrites && !current.production}
			disabled={current.production}
			onchange={(e) => save({ agentWrites: e.currentTarget.checked })}
		/>
		<span>
			<strong>AI agents may change it</strong>
			<small>
				{#if current.production}
					Never on a production context: agents only read here.
				{:else}
					Agents connected over MCP may scale, restart, set images and delete here. You allow each change in Kubri.
				{/if}
			</small>
		</span>
	</label>
</div>

<style>
	.form {
		display: grid;
		gap: 12px;
	}
	.field {
		display: grid;
		gap: 5px;
		font-size: 12px;
		color: var(--text-muted);
	}
	.field input,
	.field select {
		font-size: 13px;
	}
	.colors {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 6px;
	}
	.swatch {
		width: 22px;
		height: 22px;
		padding: 0;
		border-radius: 50%;
		border: 2px solid var(--surface-1);
		background: var(--swatch);
		box-shadow: 0 0 0 1px var(--border);
	}
	.swatch.none {
		background: linear-gradient(135deg, var(--surface-1) 45%, var(--bad) 45% 55%, var(--surface-1) 55%);
	}
	.swatch[aria-checked='true'] {
		box-shadow: 0 0 0 2px var(--text);
	}
	.custom {
		width: 86px;
		font-family: var(--mono);
		font-size: 12px;
	}
	.check {
		display: grid;
		grid-template-columns: auto 1fr;
		gap: 8px;
		align-items: start;
		font-size: 13px;
	}
	.check input {
		margin-top: 3px;
	}
	.check span {
		display: grid;
		gap: 1px;
	}
	.check small {
		color: var(--text-muted);
		font-size: 12px;
	}
</style>
