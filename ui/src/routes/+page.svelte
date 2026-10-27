<script lang="ts">
	import logo from '#lib/assets/logo.svg';
	import ContextSwitcher from '#lib/components/ContextSwitcher.svelte';
	import NamespacePicker from '#lib/components/NamespacePicker.svelte';
	import ResourceTable from '#lib/components/ResourceTable.svelte';
	import Sidebar from '#lib/components/Sidebar.svelte';
	import { app } from '#lib/state.svelte.js';

	$effect(() => {
		app.load();
	});
</script>

<div class="shell">
	<header>
		<div class="brand">
			<img src={logo} alt="" width="26" height="26" />
			<span>Kubri</span>
		</div>
		<ContextSwitcher />
		<NamespacePicker />
	</header>

	{#if app.error}
		<div class="error">
			<span>{app.error}</span>
			<button onclick={() => (app.error = null)} aria-label="Dismiss">×</button>
		</div>
	{/if}

	<main>
		<Sidebar />
		{#if app.selected}
			{#key app.cluster?.context}
				<ResourceTable resource={app.selected} namespace={app.namespace} />
			{/key}
		{:else}
			<div class="placeholder">
				{#if app.contexts.length === 0 && !app.error}
					Looking for your kubeconfig…
				{:else if !app.cluster}
					Pick a context to connect to a cluster.
				{:else}
					Pick a resource on the left.
				{/if}
			</div>
		{/if}
	</main>
</div>

<style>
	.shell {
		display: grid;
		grid-template-rows: auto auto 1fr;
		height: 100vh;
	}
	header {
		display: flex;
		align-items: center;
		gap: 20px;
		padding: 10px 16px;
		border-bottom: 1px solid var(--border);
		background: var(--surface-1);
	}
	.brand {
		display: flex;
		align-items: center;
		gap: 8px;
		width: 208px;
		font-size: 17px;
		font-weight: 700;
		letter-spacing: -0.01em;
	}
	.error {
		display: flex;
		justify-content: space-between;
		gap: 12px;
		padding: 8px 16px;
		background: color-mix(in srgb, var(--bad) 14%, var(--surface-1));
		color: var(--bad);
	}
	.error button {
		border: none;
		background: none;
		font-size: 18px;
		line-height: 1;
	}
	main {
		display: grid;
		grid-template-columns: 240px 1fr;
		min-height: 0;
	}
	.placeholder {
		display: grid;
		place-items: center;
		color: var(--text-muted);
	}
</style>
