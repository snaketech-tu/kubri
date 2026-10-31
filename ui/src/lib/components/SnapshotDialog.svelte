<script lang="ts">
	import { isTauri } from '@tauri-apps/api/core';
	import { untrack } from 'svelte';
	import { app, type Dialog } from '../state.svelte';
	import { plainOptions } from '../select';
	import Icon from './Icon.svelte';
	import Select from './Select.svelte';

	let { dialog }: { dialog: Extract<Dialog, { kind: 'snapshot' }> } = $props();

	let box = $state<HTMLDialogElement>();
	let picker = $state<HTMLInputElement>();
	// Read once: the dialog closes by clearing it.
	const start = untrack(() => dialog.namespace);
	let scope = $state<'namespace' | 'cluster'>(start === null ? 'cluster' : 'namespace');
	let namespace = $state(start ?? app.cluster?.defaultNamespace ?? 'default');
	const namespaces = $derived([...new Set([namespace, ...app.namespaces])].sort());
	const desktop = isTauri();

	$effect(() => {
		const previous = document.activeElement as HTMLElement | null;
		box?.showModal();
		return () => previous?.focus();
	});

	function close() {
		app.dialog = null;
	}

	function save() {
		close();
		app.takeSnapshot(scope === 'cluster' ? null : namespace);
	}

	function open(file: File | undefined) {
		if (!file) return;
		close();
		app.openSnapshot(file);
	}
</script>

<dialog
	class="dialog"
	aria-label="Snapshots"
	bind:this={box}
	oncancel={(e) => {
		e.preventDefault();
		close();
	}}
>
	<h2>Snapshots</h2>
	<p class="muted">
		A snapshot saves what Kubri shows to one file. It opens in Kubri on any computer, without access to the cluster, read-only and
		labeled as a snapshot: for "send me what you see".
	</p>

	<section>
		<h3>Save a snapshot</h3>
		{#if app.snapshot}
			<p class="muted">This is a snapshot already. Switch to a context to save one.</p>
		{:else}
			<div class="scope" role="radiogroup" aria-label="What to save">
				<label>
					<input type="radio" bind:group={scope} value="namespace" />
					Namespace
					<Select bind:value={namespace} options={plainOptions(namespaces)} disabled={scope !== 'namespace'} aria-label="Namespace" compact style="max-width: 260px" />
				</label>
				<label>
					<input type="radio" bind:group={scope} value="cluster" />
					The whole cluster
				</label>
			</div>
			<ul class="contents">
				<li>
					Every type you may list{scope === 'namespace' ? ' in the namespace, with nodes, storage and cluster roles' : ''}, with the
					columns of the lists, so Pulses, XRay, the graph and the sanitizer work on it
				</li>
				<li>Events, and the last 500 log lines of up to 300 containers</li>
				<li>Secrets with their keys but without values; Helm releases without their values and manifests</li>
			</ul>
			<div class="buttons">
				<button class="primary" onclick={save}>
					<Icon name="download" size={15} />
					{desktop ? 'Save to Downloads' : 'Download'}
				</button>
			</div>
		{/if}
	</section>

	<section>
		<h3>Open a snapshot</h3>
		<p class="muted">It takes the place of the cluster until you switch to a context.</p>
		<div class="buttons">
			<button onclick={() => picker?.click()}>Open a file…</button>
			<input bind:this={picker} type="file" accept=".kubri,.gz,.json" hidden onchange={(e) => open(e.currentTarget.files?.[0])} />
		</div>
	</section>

	<div class="buttons end">
		<button onclick={close}>Close</button>
	</div>
</dialog>

<style>
	.dialog {
		position: fixed;
		inset: 0;
		z-index: 51;
		width: min(560px, calc(100vw - 32px));
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
		margin: 0 0 8px;
		font-size: 17px;
	}
	h3 {
		margin: 0 0 8px;
		font-size: 14px;
	}
	section {
		margin-top: 16px;
		padding-top: 14px;
		border-top: 1px solid var(--border);
	}
	.muted {
		margin: 0 0 10px;
		color: var(--text-muted);
	}
	.scope {
		display: grid;
		gap: 8px;
	}
	.scope label {
		display: flex;
		align-items: center;
		gap: 8px;
	}
	.contents {
		margin: 12px 0 0;
		padding-left: 18px;
		color: var(--text-muted);
		font-size: 12.5px;
		line-height: 1.5;
	}
	.buttons {
		display: flex;
		gap: 8px;
		margin-top: 12px;
	}
	.buttons.end {
		justify-content: flex-end;
		margin-top: 18px;
	}
	button {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		padding: 7px 12px;
		border: 1px solid var(--border);
		border-radius: 8px;
		background: var(--surface-1);
		color: var(--text);
	}
	.primary {
		border-color: var(--accent);
		background: var(--accent);
		color: var(--on-accent);
	}
</style>
