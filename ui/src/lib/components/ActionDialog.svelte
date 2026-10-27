<script lang="ts">
	import { deleteObjects, patchMetadata, restart, scale } from '../actions';
	import { backend, type DeleteOptions } from '../api';
	import { errorMessage } from '../format';
	import { app, type Dialog } from '../state.svelte';

	let { dialog }: { dialog: Dialog } = $props();

	let busy = $state(false);
	let error = $state<string | null>(null);
	let confirmButton = $state<HTMLButtonElement>();
	let firstField = $state<HTMLInputElement | HTMLTextAreaElement>();

	// Delete
	let force = $state(false);
	let propagation = $state<NonNullable<DeleteOptions['propagation']>>('background');

	// Scale
	let replicas = $state<number | null>(null);

	// Labels and annotations, one key=value per line
	let labelsText = $state('');
	let annotationsText = $state('');
	let original = { labels: {} as Record<string, string>, annotations: {} as Record<string, string> };

	$effect(() => {
		if (dialog.kind === 'scale' || dialog.kind === 'labels') {
			backend
				.getObject(dialog.target)
				.then((obj) => {
					if (dialog.kind === 'scale') {
						replicas = typeof obj.spec?.replicas === 'number' ? obj.spec.replicas : 1;
					} else {
						original = { labels: obj.metadata.labels ?? {}, annotations: obj.metadata.annotations ?? {} };
						labelsText = toLines(original.labels);
						annotationsText = toLines(original.annotations);
					}
					queueMicrotask(() => firstField?.focus());
				})
				.catch((err) => (error = errorMessage(err)));
		} else {
			queueMicrotask(() => confirmButton?.focus());
		}
	});

	function toLines(map: Record<string, string>) {
		return Object.entries(map)
			.filter(([k]) => k !== 'kubectl.kubernetes.io/last-applied-configuration')
			.map(([k, v]) => `${k}=${v}`)
			.join('\n');
	}

	function fromLines(text: string): Record<string, string> {
		const out: Record<string, string> = {};
		for (const line of text.split('\n')) {
			const trimmed = line.trim();
			if (!trimmed) continue;
			const i = trimmed.indexOf('=');
			if (i <= 0) throw new Error(`Expected key=value, got "${trimmed}"`);
			out[trimmed.slice(0, i).trim()] = trimmed.slice(i + 1).trim();
		}
		return out;
	}

	/** A merge patch that sets changed keys and removes deleted ones. */
	function diff(before: Record<string, string>, after: Record<string, string>) {
		const patch: Record<string, string | null> = {};
		for (const key of Object.keys(before)) {
			if (!(key in after) && key !== 'kubectl.kubernetes.io/last-applied-configuration') patch[key] = null;
		}
		for (const [key, value] of Object.entries(after)) {
			if (before[key] !== value) patch[key] = value;
		}
		return patch;
	}

	function close() {
		app.dialog = null;
	}

	async function confirm() {
		busy = true;
		error = null;
		let ok = false;
		try {
			switch (dialog.kind) {
				case 'delete':
					ok = await deleteObjects(dialog.resource, dialog.targets, {
						force,
						gracePeriodSeconds: force ? 0 : null,
						propagation
					});
					break;
				case 'scale':
					if (replicas === null || replicas < 0 || !Number.isInteger(replicas)) {
						throw new Error('Replicas must be a whole number of 0 or more');
					}
					ok = await scale(dialog.target, replicas);
					break;
				case 'restart':
					ok = await restart(dialog.target);
					break;
				case 'labels':
					ok = await patchMetadata(
						dialog.target,
						diff(original.labels, fromLines(labelsText)),
						diff(original.annotations, fromLines(annotationsText))
					);
					break;
			}
		} catch (err) {
			error = errorMessage(err);
		}
		busy = false;
		if (ok) close();
	}

	function onkeydown(e: KeyboardEvent) {
		if (e.key === 'Escape') {
			e.preventDefault();
			close();
		} else if (e.key === 'Enter' && (e.ctrlKey || e.metaKey || !(e.target instanceof HTMLTextAreaElement))) {
			if (e.target instanceof HTMLButtonElement && e.target !== confirmButton) return;
			e.preventDefault();
			confirm();
		}
	}

	function name(t: { namespace: string | null; name: string }) {
		return t.namespace ? `${t.namespace}/${t.name}` : t.name;
	}

	const title = $derived(
		{
			delete: `Delete ${dialog.resource.kind}`,
			scale: `Scale ${dialog.resource.kind}`,
			restart: `Restart ${dialog.resource.kind}`,
			labels: 'Labels and annotations'
		}[dialog.kind]
	);
</script>

<div class="backdrop" onclick={close} role="presentation"></div>
<div class="dialog" role="dialog" aria-modal="true" aria-label={title} tabindex="-1" {onkeydown}>
	<h2>{title}</h2>

	{#if dialog.kind === 'delete'}
		<p>
			{dialog.targets.length === 1 ? 'This deletes' : `This deletes ${dialog.targets.length} objects:`}
		</p>
		<ul class="targets">
			{#each dialog.targets.slice(0, 6) as t (name(t))}
				<li>{name(t)}</li>
			{/each}
			{#if dialog.targets.length > 6}<li class="muted">and {dialog.targets.length - 6} more</li>{/if}
		</ul>
		<label class="row"><input type="checkbox" bind:checked={force} /> Force (no grace period)</label>
		<label class="row">
			Dependents
			<select bind:value={propagation}>
				<option value="background">delete in background</option>
				<option value="foreground">delete first (foreground)</option>
				<option value="orphan">keep (orphan)</option>
			</select>
		</label>
	{:else if dialog.kind === 'scale'}
		<p>Set the number of replicas for <strong>{name(dialog.target)}</strong>.</p>
		<input
			class="replicas"
			type="number"
			min="0"
			bind:value={replicas}
			bind:this={firstField}
			disabled={replicas === null}
		/>
	{:else if dialog.kind === 'restart'}
		<p>Restart all pods of <strong>{name(dialog.target)}</strong> with a rolling update?</p>
	{:else if dialog.kind === 'labels'}
		<p class="muted">One <code>key=value</code> per line. Remove a line to delete it.</p>
		<label class="field">
			Labels
			<textarea rows="5" bind:value={labelsText} bind:this={firstField} spellcheck="false"></textarea>
		</label>
		<label class="field">
			Annotations
			<textarea rows="5" bind:value={annotationsText} spellcheck="false"></textarea>
		</label>
	{/if}

	{#if error}<p class="error">{error}</p>{/if}

	<div class="buttons">
		<button class="secondary" onclick={close}>Cancel</button>
		<button class="primary" class:danger={dialog.kind === 'delete'} bind:this={confirmButton} disabled={busy} onclick={confirm}>
			{busy ? 'Working…' : { delete: 'Delete', scale: 'Scale', restart: 'Restart', labels: 'Save' }[dialog.kind]}
		</button>
	</div>
</div>

<style>
	.backdrop {
		position: fixed;
		inset: 0;
		z-index: 50;
		background: rgb(0 0 0 / 35%);
	}
	.dialog {
		position: fixed;
		top: 50%;
		left: 50%;
		z-index: 51;
		width: min(480px, calc(100vw - 32px));
		max-height: 85vh;
		overflow: auto;
		padding: 20px 22px;
		transform: translate(-50%, -50%);
		border: 1px solid var(--border);
		border-radius: 12px;
		background: var(--surface-1);
		box-shadow: var(--shadow);
		outline: none;
	}
	h2 {
		margin: 0 0 12px;
		font-size: 17px;
	}
	p {
		margin: 0 0 10px;
	}
	.targets {
		margin: 0 0 14px;
		padding-left: 18px;
		font-family: var(--mono);
		font-size: 12.5px;
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
	textarea {
		padding: 8px 10px;
		border: 1px solid var(--border);
		border-radius: 6px;
		background: var(--bg);
		color: var(--text);
		font-family: var(--mono);
		font-size: 12px;
		resize: vertical;
	}
	.replicas {
		width: 120px;
		font-size: 18px;
	}
	.error {
		color: var(--bad);
	}
	.muted {
		color: var(--text-muted);
	}
	.buttons {
		display: flex;
		justify-content: flex-end;
		gap: 8px;
		margin-top: 18px;
	}
	.buttons button {
		padding: 7px 14px;
		border-radius: 7px;
	}
	.primary {
		border: 1px solid var(--accent);
		background: var(--accent);
		color: white;
	}
	.primary.danger {
		border-color: var(--bad);
		background: var(--bad);
	}
	.secondary {
		border: 1px solid var(--border);
		background: var(--surface-1);
	}
</style>
