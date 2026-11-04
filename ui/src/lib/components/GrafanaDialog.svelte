<script lang="ts">
	import { lightDismiss } from '../dismiss';
	import { untrack } from 'svelte';
	import { app, type Dialog } from '../state.svelte';
	import DashboardList from './DashboardList.svelte';

	let { dialog }: { dialog: Extract<Dialog, { kind: 'grafana' }> } = $props();
	// Read once: the dialog closes by clearing it.
	const { target } = untrack(() => dialog);

	let box = $state<HTMLDialogElement>();

	$effect(() => {
		const previous = document.activeElement as HTMLElement | null;
		box?.showModal();
		return () => previous?.focus();
	});

	function close() {
		app.dialog = null;
	}

	function setUp() {
		close();
		app.showIntegrations();
	}
</script>

<dialog
	{@attach lightDismiss}
	class="dialog"
	aria-label="Grafana for {target.name}"
	bind:this={box}
	oncancel={(e) => {
		e.preventDefault();
		close();
	}}
>
	<h2>Grafana for {target.name}</h2>
	<DashboardList
		{target}
		title="Dashboards and links"
		empty="Neither Grafana nor the links of this context have a dashboard for this object."
	/>
	<div class="buttons">
		<button class="secondary" onclick={setUp}>Set up links…</button>
		<button class="primary" onclick={close}>Close</button>
	</div>
</dialog>

<style>
	.dialog {
		position: fixed;
		inset: 0;
		z-index: 51;
		width: min(480px, calc(100vw - 32px));
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
		margin: 0 0 12px;
		font-size: 17px;
		overflow-wrap: anywhere;
	}
	.buttons {
		display: flex;
		flex-wrap: wrap;
		justify-content: flex-end;
		gap: 8px;
		margin-top: 18px;
	}
	.buttons button {
		padding: 8px 14px;
		border-radius: 8px;
	}
	.primary {
		border: 1px solid var(--accent);
		background: var(--accent);
		color: var(--on-accent);
	}
	.secondary {
		border: 1px solid var(--border);
		background: var(--surface-1);
		color: var(--text);
	}
</style>
