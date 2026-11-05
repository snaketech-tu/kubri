<script lang="ts">
	import { untrack } from 'svelte';
	import { backend, type LocalClusters, type LocalClusterTool } from '../api';
	import { lightDismiss } from '../dismiss';
	import { errorMessage } from '../format';
	import { retryKubeconfig } from '../start';
	import { app } from '../state.svelte';
	import { mountTerminal, TERMINAL_BACKGROUND } from '../terminal';

	let { onclose }: { onclose: () => void } = $props();

	let box = $state<HTMLDialogElement>();
	let element = $state<HTMLDivElement>();
	let info = $state<LocalClusters | null>(null);
	let problem = $state<string | null>(null);
	let tool = $state<LocalClusterTool | null>(null);
	let status = $state<'choose' | 'starting' | 'running' | 'connecting' | 'failed'>('choose');
	/** Counts the runs, so that Try again starts the tool afresh. */
	let run = $state(0);

	const chosen = $derived(info?.tools.find((t) => t.tool === tool) ?? null);
	const busy = $derived(status === 'starting' || status === 'running' || status === 'connecting');

	$effect(() => {
		const previous = document.activeElement as HTMLElement | null;
		box?.showModal();
		return () => previous?.focus();
	});

	$effect(() => {
		backend.localClusters().then(
			(found) => {
				info = found;
				tool = found.tools.find((t) => t.found)?.tool ?? null;
			},
			(err) => (problem = errorMessage(err))
		);
	});

	$effect(() => {
		if (!run || !element || !tool) return;
		const request = untrack(() => ({ tool: tool!, command: chosen?.command ?? tool! }));
		const { fontSize, fontFamily } = untrack(() => app.settings.terminal ?? { fontSize: 13, fontFamily: '' });
		return mountTerminal(
			element,
			(term) => {
				term.write(`\x1b[2m$ ${request.command}\x1b[0m\r\n`);
				return backend.createLocalCluster({ tool: request.tool, cols: term.cols, rows: term.rows }, (event) => {
					switch (event.type) {
						case 'ready':
							status = 'running';
							break;
						case 'data':
							term.write(event.data);
							break;
						case 'exit':
							if (event.message === null) {
								connect();
							} else {
								status = 'failed';
								term.write(`\r\n\x1b[2m[${event.message}]\x1b[0m\r\n`);
							}
							break;
					}
				});
			},
			{ fontSize, fontFamily }
		);
	});

	function start() {
		status = 'starting';
		run += 1;
	}

	/** The tool added the context and made it the current one: reading the kubeconfig again connects to it. */
	async function connect() {
		status = 'connecting';
		await retryKubeconfig();
		onclose();
	}

	function close() {
		// Stopping a run halfway would leave half a cluster behind; the tool cleans up on failure itself.
		if (status === 'connecting') return;
		onclose();
	}
</script>

<dialog
	{@attach lightDismiss}
	class="dialog"
	class:wide={run > 0}
	aria-label="Start a cluster on this computer"
	bind:this={box}
	oncancel={(e) => {
		e.preventDefault();
		if (!busy) close();
	}}
>
	<h2>Start a cluster on this computer</h2>
	<p class="muted">
		kind and k3d run a small Kubernetes cluster in containers on this computer. Kubri runs the one you pick, shows what it does, and
		connects to the new cluster when it is ready. This takes a minute or two the first time.
	</p>

	{#if problem}
		<p class="problem">{problem}</p>
	{:else if !info}
		<p class="muted">Looking for kind and k3d…</p>
	{:else if !info.allowed}
		<p class="problem">{info.reason}</p>
	{:else if run === 0}
		<div class="tools" role="radiogroup" aria-label="Tool">
			{#each info.tools as option (option.tool)}
				<label class:missing={!option.found}>
					<input type="radio" bind:group={tool} value={option.tool} disabled={!option.found} />
					<span class="name">{option.tool}</span>
					{#if option.found}
						<code>{option.command}</code>
					{:else}
						<span class="muted">Not installed. <a href={option.install} target="_blank" rel="noreferrer">How to install {option.tool}</a></span>
					{/if}
				</label>
			{/each}
		</div>
		{#if !info.containers}
			<p class="hint">Docker was not found. Install and start Docker first: kind and k3d run the cluster in its containers.</p>
		{/if}
		{#if !info.tools.some((t) => t.found)}
			<p class="hint">Install kind or k3d, then open this again.</p>
		{/if}
	{:else}
		<div class="screen" style:background={TERMINAL_BACKGROUND}><div class="host" bind:this={element}></div></div>
		<p class="status" class:failed={status === 'failed'} role="status">
			{status === 'starting'
				? `Starting ${tool}…`
				: status === 'running'
					? `${tool} is making the cluster ${chosen?.context ?? ''}…`
					: status === 'connecting'
						? `Connecting to ${chosen?.context ?? 'the new cluster'}…`
						: `${tool} could not make the cluster. Its output above says why.`}
		</p>
	{/if}

	<div class="buttons">
		{#if run === 0}
			<button onclick={close}>Cancel</button>
			<button class="primary" disabled={!info?.allowed || !chosen?.found} onclick={start}>Start the cluster</button>
		{:else if status === 'failed'}
			<button onclick={close}>Close</button>
			<button class="primary" onclick={start}>Try again</button>
		{:else}
			<button onclick={close} disabled={status === 'connecting'} title="Stops the tool">Stop</button>
		{/if}
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
		text-align: left;
	}
	.dialog.wide {
		width: min(820px, calc(100vw - 32px));
	}
	.dialog::backdrop {
		background: rgb(16 21 34 / 45%);
	}
	h2 {
		margin: 0 0 8px;
		font-size: 17px;
	}
	.muted {
		margin: 0 0 12px;
		color: var(--text-muted);
	}
	.tools {
		display: grid;
		gap: 8px;
		margin: 4px 0 12px;
	}
	.tools label {
		display: grid;
		grid-template-columns: auto 48px minmax(0, 1fr);
		align-items: baseline;
		gap: 10px;
		padding: 10px 12px;
		border: 1px solid var(--border);
		border-radius: 8px;
	}
	.tools label:has(input:checked) {
		border-color: var(--accent);
	}
	.tools label.missing .name {
		color: var(--text-muted);
	}
	.tools .muted {
		margin: 0;
	}
	.name {
		font-weight: 600;
	}
	code {
		overflow-wrap: anywhere;
		font-family: var(--mono);
		font-size: 12.5px;
		color: var(--text-muted);
	}
	.problem,
	.hint {
		margin: 0 0 12px;
		padding: 8px 12px;
		border-radius: 8px;
		font-size: 13px;
	}
	.problem {
		background: color-mix(in srgb, var(--bad) 8%, transparent);
		color: var(--bad);
	}
	.hint {
		background: color-mix(in srgb, var(--warn) 10%, transparent);
	}
	.screen {
		height: min(360px, 50dvh);
		padding: 8px;
		border-radius: 8px;
	}
	.host {
		width: 100%;
		height: 100%;
	}
	.status {
		margin: 10px 0 0;
		color: var(--text-muted);
	}
	.status.failed {
		color: var(--bad);
	}
	.buttons {
		display: flex;
		justify-content: flex-end;
		gap: 8px;
		margin-top: 16px;
	}
	.buttons button {
		padding: 7px 12px;
		border: 1px solid var(--border);
		border-radius: 8px;
		background: var(--surface-1);
		color: var(--text);
	}
	.buttons .primary {
		border-color: var(--accent);
		background: var(--accent);
		color: var(--on-accent);
	}
	.buttons button:disabled {
		opacity: 0.5;
	}
</style>
