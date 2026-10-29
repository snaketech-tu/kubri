<script lang="ts">
	import { fieldAt, fieldsAt, resolve, typeName, type FieldPath, type KindSchema } from '../schema';

	let { schema, path }: { schema: KindSchema; path: FieldPath } = $props();

	/** The deepest part of the path the schema knows: free-form keys such as a label's have no docs. */
	const known = $derived.by(() => {
		let depth = path.length;
		while (depth > 0 && !fieldAt(schema, path.slice(0, depth))) depth--;
		return path.slice(0, depth);
	});
	const node = $derived(fieldAt(schema, known));
	const resolved = $derived(resolve(schema, node));
	const kind = $derived(schema.root.split('.').pop() ?? '');
	const where = $derived(
		known.reduce<string>((text, step) => (typeof step === 'number' ? `${text}[${step}]` : `${text}.${step}`), kind)
	);
	const fields = $derived(fieldsAt(schema, known));
	const choices = $derived(resolved?.enum?.map(String) ?? []);
</script>

<section class="help" aria-label="Field help" aria-live="polite">
	<div class="head">
		<code class="path">{where}</code>
		<span class="type">{typeName(schema, node)}</span>
	</div>
	{#if resolved?.description}
		<p>{resolved.description}</p>
	{:else}
		<p class="muted">The schema has no description for this field.</p>
	{/if}
	{#if choices.length}
		<p class="muted">One of {#each choices as choice, i (choice)}{i ? ', ' : ''}<code>{choice}</code>{/each}.</p>
	{/if}
	{#if fields.length}
		<ul>
			{#each fields as field (field.name)}
				<li title={field.description}>
					<code>{field.name}</code>
					<span class="type">{field.type}</span>{#if field.required}<span class="required">required</span>{/if}
				</li>
			{/each}
		</ul>
	{/if}
</section>

<style>
	/* A fixed height, so the editor above does not jump as the cursor moves. */
	.help {
		height: min(9.5em + 40px, 34%);
		flex: none !important;
		overflow: auto;
		padding: 8px 14px 10px;
		border-top: 1px solid var(--border);
		background: var(--surface-2);
		font-size: 12px;
		line-height: 1.45;
	}
	.head {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		gap: 8px;
		margin-bottom: 4px;
	}
	.path {
		font-weight: 600;
		overflow-wrap: anywhere;
	}
	code {
		font-family: var(--mono);
	}
	.type {
		color: var(--text-muted);
		font-family: var(--mono);
	}
	p {
		margin: 0 0 6px;
		white-space: pre-line;
	}
	.muted {
		color: var(--text-muted);
	}
	ul {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
		gap: 2px 16px;
		margin: 6px 0 0;
		padding: 0;
		list-style: none;
	}
	li {
		overflow: hidden;
		white-space: nowrap;
		text-overflow: ellipsis;
	}
	li .type {
		margin-left: 6px;
	}
	.required {
		margin-left: 6px;
		color: var(--warn);
	}
</style>
