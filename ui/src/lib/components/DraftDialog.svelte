<script lang="ts">
	import { drafts } from '../navigation.svelte';
	let box = $state<HTMLDialogElement>();
	$effect(() => {
		if (drafts.pending) box?.showModal();
		else box?.close();
	});
</script>

<dialog
	bind:this={box}
	oncancel={(e) => {
		e.preventDefault();
		if (!drafts.saving) drafts.pending = null;
	}}
	aria-labelledby="draft-title"
>
	<h2 id="draft-title">Keep your YAML changes?</h2>
	<p>Your changes have not been saved. Save them before continuing, or keep editing.</p>
	<div class="buttons">
		<button disabled={drafts.saving} onclick={() => (drafts.pending = null)}>Keep editing</button>
		<button disabled={drafts.saving} onclick={() => drafts.proceed('discard')}>Discard changes</button>
		<button class="primary" disabled={drafts.saving} onclick={() => drafts.proceed('save')}
			>{drafts.saving ? 'Saving…' : 'Save and continue'}</button
		>
	</div>
</dialog>

<style>
	dialog {
		width: min(480px, calc(100vw - 32px));
		border: 1px solid var(--border);
		border-radius: 12px;
		padding: 24px;
		background: var(--surface-1);
		color: var(--text);
		box-shadow: var(--shadow);
	}
	dialog::backdrop {
		background: #10152288;
	}
	h2 {
		margin: 0 0 12px;
		font-size: 20px;
	}
	p {
		color: var(--text-muted);
		line-height: 1.6;
	}
	.buttons {
		display: flex;
		justify-content: flex-end;
		gap: 8px;
		flex-wrap: wrap;
		margin-top: 24px;
	}
	button {
		padding: 8px 12px;
		border: 1px solid var(--border);
		border-radius: 7px;
		background: var(--surface-2);
	}
	.primary {
		background: var(--accent);
		color: var(--on-accent);
		border-color: var(--accent);
	}
</style>
