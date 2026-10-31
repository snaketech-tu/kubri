<script lang="ts">
	import { FORMS, fieldText, fieldValue, getPath, rename, setPath, type FormField } from '../forms';
	import { plainOptions } from '../select';
	import Icon from './Icon.svelte';
	import Select from './Select.svelte';

	let {
		object,
		onchange
	}: {
		/** The one object in the editor. */
		object: Record<string, any>;
		/** A changed copy, for the editor to show. */
		onchange: (object: Record<string, any>) => void;
	} = $props();

	const fields = $derived(FORMS[object.kind] ?? []);

	function change(field: FormField, text: string) {
		const copy = structuredClone($state.snapshot(object));
		if (field.path.join('.') === 'metadata.name') rename(copy, text.trim());
		else setPath(copy, field.path, fieldValue(field, text));
		onchange(copy);
	}

	function entries(field: FormField): [string, string][] {
		const map = getPath(object, field.path);
		return map && typeof map === 'object' ? Object.entries(map).map(([k, v]) => [k, String(v ?? '')]) : [];
	}

	/** Writes a map back from its rows, keeping their order. */
	function setEntries(field: FormField, rows: [string, string][]) {
		const copy = structuredClone($state.snapshot(object));
		const map = Object.fromEntries(rows.filter(([k]) => k !== ''));
		setPath(copy, field.path, rows.length ? map : undefined);
		onchange(copy);
	}

	function editEntry(field: FormField, index: number, key: string | null, value: string | null) {
		const rows = entries(field);
		const [k, v] = rows[index];
		rows[index] = [key ?? k, value ?? v];
		setEntries(field, rows);
	}

	function addEntry(field: FormField) {
		const rows = entries(field);
		let key = 'key';
		for (let i = 2; rows.some(([k]) => k === key); i++) key = `key${i}`;
		setEntries(field, [...rows, [key, '']]);
	}
</script>

<aside class="form" aria-label="{object.kind} form">
	<h3>{object.kind}</h3>
	<p class="lead">Changes here go into the YAML, and edits in the YAML show here.</p>
	{#each fields as field (field.path.join('.'))}
		{#if field.kind === 'map'}
			<fieldset>
				<legend>{field.label}</legend>
				{#each entries(field) as [key, value], i (i)}
					<div class="row">
						<input
							value={key}
							aria-label="{field.label} key {i + 1}"
							spellcheck="false"
							onchange={(e) => editEntry(field, i, e.currentTarget.value, null)}
						/>
						<input
							value={value}
							aria-label="{field.label} value {i + 1}"
							spellcheck="false"
							onchange={(e) => editEntry(field, i, null, e.currentTarget.value)}
						/>
						<button
							class="remove"
							onclick={() => setEntries(field, entries(field).filter((_, j) => j !== i))}
							aria-label="Remove {key}"
							title="Remove"><Icon name="close" size={13} /></button
						>
					</div>
				{/each}
				<button class="add" onclick={() => addEntry(field)}>Add</button>
				{#if field.hint}<p class="hint">{field.hint}</p>{/if}
			</fieldset>
		{:else}
			<label>
				<span>{field.label}</span>
				{#if field.kind === 'select'}
					<Select value={fieldText(object, field) || field.options?.[0]} options={plainOptions(field.options ?? [])} onchange={(value) => change(field, value)} mono />
				{:else}
					<input
						type={field.kind === 'number' ? 'number' : 'text'}
						value={fieldText(object, field)}
						placeholder={field.placeholder}
						spellcheck="false"
						onchange={(e) => change(field, e.currentTarget.value)}
					/>
				{/if}
			</label>
			{#if field.hint}<p class="hint">{field.hint}</p>{/if}
		{/if}
	{/each}
</aside>

<style>
	.form {
		display: flex;
		flex-direction: column;
		gap: 6px;
		width: 320px;
		flex: none;
		padding: 14px 16px;
		border-left: 1px solid var(--border);
		background: var(--surface-1);
		overflow: auto;
		font-size: 12.5px;
	}
	h3 {
		margin: 0;
		font-size: 14px;
	}
	.lead {
		margin: 0 0 8px;
		color: var(--text-muted);
	}
	label {
		display: grid;
		gap: 4px;
		margin-top: 6px;
	}
	label span,
	legend {
		color: var(--text-muted);
		font-weight: 600;
	}
	input {
		min-width: 0;
		padding: 6px 8px;
		border: 1px solid var(--border);
		border-radius: 6px;
		background: var(--surface-1);
		color: var(--text);
		font-family: var(--mono);
		font-size: 12.5px;
	}
	fieldset {
		display: grid;
		gap: 6px;
		margin: 6px 0 0;
		padding: 0;
		border: none;
	}
	legend {
		margin-bottom: 4px;
		padding: 0;
	}
	.row {
		display: grid;
		grid-template-columns: 1fr 1fr auto;
		gap: 6px;
	}
	.remove {
		display: grid;
		place-items: center;
		width: 28px;
		border: 1px solid var(--border);
		border-radius: 6px;
		background: var(--surface-1);
		color: var(--text-muted);
	}
	.add {
		justify-self: start;
		padding: 4px 10px;
		border: 1px solid var(--border);
		border-radius: 6px;
		background: var(--surface-1);
		font-size: 12px;
	}
	.hint {
		margin: 0;
		color: var(--text-muted);
		font-size: 12px;
	}
	@media (max-width: 900px) {
		.form {
			width: auto;
			border-left: none;
			border-top: 1px solid var(--border);
		}
	}
</style>
