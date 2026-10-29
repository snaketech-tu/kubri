<script lang="ts">
	import { untrack } from 'svelte';
	import { copyLines } from '../actions';
	import { app, type Dialog } from '../state.svelte';

	let { dialog }: { dialog: Extract<Dialog, { kind: 'kubectl' }> } = $props();
	// Read once: the dialog closes by clearing it.
	const { title, commands } = untrack(() => dialog);

	let box = $state<HTMLDialogElement>();

	$effect(() => {
		const previous = document.activeElement as HTMLElement | null;
		box?.showModal();
		return () => previous?.focus();
	});

	function close() {
		app.dialog = null;
	}
</script>

<dialog
	class="dialog"
	aria-label="Copy as kubectl"
	bind:this={box}
	oncancel={(e) => {
		e.preventDefault();
		close();
	}}
>
	<h2>Copy as kubectl</h2>
	<p class="muted">
		What Kubri does on <strong>{title}</strong>, as commands for a terminal or a runbook. Values you choose in a dialog, like the
		number of replicas, show in capitals.
	</p>
	<ul>
		{#each commands as { label, command } (label)}
			<li>
				<div class="head">
					<span>{label}</span>
					<button class="small" onclick={() => copyLines([command], ['command', 'commands'])}>Copy</button>
				</div>
				<pre>{command}</pre>
			</li>
		{/each}
	</ul>
	<div class="buttons">
		<button class="primary" onclick={close}>Close</button>
	</div>
</dialog>

<style>
	.dialog {
		position: fixed;
		inset: 0;
		z-index: 51;
		width: min(640px, calc(100vw - 32px));
		max-height: 85dvh;
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
		margin: 0 0 10px;
		font-size: 17px;
	}
	.muted {
		margin: 0 0 12px;
		color: var(--text-muted);
	}
	ul {
		display: grid;
		gap: 8px;
		margin: 0;
		padding: 0;
		list-style: none;
	}
	li {
		border: 1px solid var(--border);
		border-radius: 8px;
		overflow: hidden;
	}
	.head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 4px 6px 4px 10px;
		background: var(--surface-2);
		font-size: 12.5px;
	}
	pre {
		margin: 0;
		padding: 8px 10px;
		overflow-x: auto;
		font-family: var(--mono);
		font-size: 12px;
		white-space: pre-wrap;
		word-break: break-all;
	}
	.buttons {
		display: flex;
		justify-content: flex-end;
		margin-top: 16px;
	}
	.primary {
		padding: 8px 14px;
		border: 1px solid var(--accent);
		border-radius: 8px;
		background: var(--accent);
		color: var(--on-accent);
	}
	.small {
		padding: 2px 8px;
		border: 1px solid var(--border);
		border-radius: 6px;
		background: var(--surface-1);
		font-size: 12px;
	}
</style>
