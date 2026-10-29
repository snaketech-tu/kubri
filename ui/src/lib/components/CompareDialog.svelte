<script lang="ts">
	import { untrack } from 'svelte';
	import { backend, type ObjectRef } from '../api';
	import { comparable, imageChanges } from '../compare';
	import { errorMessage } from '../format';
	import { app, type Dialog } from '../state.svelte';
	import DiffView from './DiffView.svelte';

	let { dialog }: { dialog: Extract<Dialog, { kind: 'compare' }> } = $props();
	// Read once: the dialog closes by clearing it.
	const d = untrack(() => dialog);
	const here = untrack(() => app.cluster?.context ?? '');
	const left: ObjectRef = { ...d.target, context: here };

	let box = $state<HTMLDialogElement>();
	/** Another context by default: comparing staging with production is the common case. */
	let context = $state(untrack(() => app.contexts.find((c) => c.name !== here)?.name ?? here));
	let namespace = $state(d.target.namespace ?? '');
	let name = $state(d.target.name);
	let status = $state(false);
	let busy = $state(false);
	let error = $state<string | null>(null);
	let shown = $state.raw<{ left: unknown; right: unknown; where: string } | null>(null);

	const changes = $derived(shown ? imageChanges(shown.left, shown.right) : []);
	const label = (context: string, namespace: string | null, name: string) =>
		`${app.contextName(context)}${namespace ? ` · ${namespace}` : ''} · ${name}`;

	$effect(() => {
		const previous = document.activeElement as HTMLElement | null;
		box?.showModal();
		return () => previous?.focus();
	});

	async function compare(e?: SubmitEvent) {
		e?.preventDefault();
		busy = true;
		error = null;
		const right: ObjectRef = { ...d.target, context, namespace: d.target.namespace === null ? null : namespace.trim(), name: name.trim() };
		try {
			const [a, b] = await Promise.all([
				backend.getObjectElsewhere(left),
				backend.getObjectElsewhere(right).catch((err) => {
					throw new Error(`${label(context, right.namespace, right.name)}: ${errorMessage(err)}`);
				})
			]);
			shown = { left: a, right: b, where: label(context, right.namespace, right.name) };
		} catch (err) {
			error = errorMessage(err);
			shown = null;
		} finally {
			busy = false;
		}
	}

	$effect(() => {
		untrack(() => compare());
	});
</script>

<dialog
	class="dialog"
	aria-label="Compare"
	bind:this={box}
	oncancel={(e) => {
		e.preventDefault();
		app.dialog = null;
	}}
>
	<form onsubmit={compare}>
		<h2>Compare {d.resource.kind} {d.target.name}</h2>
		<p class="this"><span>This</span><strong>{label(here, d.target.namespace, d.target.name)}</strong></p>
		<div class="with">
			<span class="side">With</span>
			<label>
				<span>Context</span>
				<select bind:value={context}>
					{#each app.contexts as c (c.name)}<option value={c.name}>{app.contextName(c.name)}</option>{/each}
				</select>
			</label>
			{#if d.target.namespace !== null}
				<label>
					<span>Namespace</span>
					<input bind:value={namespace} required spellcheck="false" />
				</label>
			{/if}
			<label>
				<span>Name</span>
				<input bind:value={name} required spellcheck="false" />
			</label>
			<button type="submit" class="primary" disabled={busy}>{busy ? 'Comparing…' : 'Compare'}</button>
		</div>
		<label class="check">
			<input type="checkbox" bind:checked={status} />
			Include status
		</label>
	</form>

	{#if error}
		<p class="error" role="alert">{error}</p>
	{:else if shown}
		{#if changes.length}
			<table class="images">
				<thead><tr><th>Container</th><th>This</th><th>{shown.where}</th></tr></thead>
				<tbody>
					{#each changes as c (c.container)}
						<tr><td>{c.container}</td><td>{c.left ?? '—'}</td><td>{c.right ?? '—'}</td></tr>
					{/each}
				</tbody>
			</table>
		{/if}
		<DiffView
			before={comparable(shown.left, status)}
			after={comparable(shown.right, status)}
			maxHeight="52dvh"
			changed="lines differ, from this to {shown.where}"
			unchanged="The same, apart from what each copy keeps of its own, such as its namespace, version and timestamps."
		/>
	{/if}
	<div class="buttons">
		<button type="button" class="secondary" onclick={() => (app.dialog = null)}>Close</button>
	</div>
</dialog>

<style>
	.dialog {
		position: fixed;
		inset: 0;
		z-index: 51;
		width: min(980px, calc(100vw - 32px));
		max-height: 90dvh;
		margin: auto;
		overflow: auto;
		padding: 20px 22px;
		border: 1px solid var(--border);
		border-radius: 12px;
		background: var(--surface-1);
		box-shadow: var(--shadow);
		color: var(--text);
	}
	.dialog::backdrop {
		background: rgb(16 21 34 / 45%);
	}
	h2 {
		margin: 0 0 12px;
		font-size: 17px;
		overflow-wrap: anywhere;
	}
	.this {
		display: flex;
		gap: 10px;
		margin: 0 0 10px;
		font-size: 13px;
		align-items: baseline;
	}
	.this span,
	.side {
		width: 44px;
		flex: none;
		color: var(--text-muted);
		font-size: 12px;
		font-weight: 600;
	}
	.this strong {
		font-family: var(--mono);
		font-weight: 600;
		overflow-wrap: anywhere;
	}
	.with {
		display: flex;
		flex-wrap: wrap;
		align-items: end;
		gap: 10px;
	}
	.with .side {
		align-self: center;
		margin-top: 16px;
	}
	.with label {
		display: flex;
		flex-direction: column;
		gap: 3px;
		font-size: 11.5px;
		font-weight: 600;
		color: var(--text-muted);
	}
	input:not([type='checkbox']),
	select {
		min-width: 0;
		width: 190px;
		padding: 6px 8px;
		border: 1px solid var(--border);
		border-radius: 6px;
		background: var(--surface-1);
		color: var(--text);
		font-family: var(--mono);
		font-size: 13px;
	}
	.check {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		margin: 10px 0 0 54px;
		font-size: 12.5px;
		color: var(--text-muted);
	}
	.check input {
		accent-color: var(--accent);
	}
	.images {
		width: 100%;
		margin: 14px 0 10px;
		border-collapse: collapse;
		font-size: 12.5px;
	}
	.images th {
		padding: 6px 8px;
		border-bottom: 1px solid var(--border);
		color: var(--text-muted);
		font-size: 11.5px;
		text-align: left;
	}
	.images td {
		padding: 6px 8px;
		border-bottom: 1px solid var(--border-subtle);
		font-family: var(--mono);
		overflow-wrap: anywhere;
	}
	.dialog :global(.diff) {
		margin-top: 14px;
	}
	.error {
		margin: 14px 0 0;
		padding: 10px 12px;
		border-radius: 8px;
		background: color-mix(in srgb, var(--bad) 10%, transparent);
		color: var(--bad);
		overflow-wrap: anywhere;
	}
	.buttons {
		display: flex;
		justify-content: flex-end;
		gap: 8px;
		margin-top: 16px;
	}
	button.primary,
	.buttons button {
		padding: 7px 14px;
		border-radius: 8px;
	}
	.primary {
		border: 1px solid var(--accent);
		background: var(--accent);
		color: var(--on-accent);
	}
	.primary:disabled {
		opacity: 0.6;
	}
	.secondary {
		border: 1px solid var(--border);
		background: var(--surface-1);
	}
</style>
