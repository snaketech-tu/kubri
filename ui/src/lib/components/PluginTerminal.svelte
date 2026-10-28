<script lang="ts">
	import { backend, type Plugin, type PluginRun } from '../api';
	import { app } from '../state.svelte';
	import { mountTerminal, TERMINAL_BACKGROUND } from '../terminal';

	let { plugin, run }: { plugin: Plugin; run: PluginRun } = $props();

	let element = $state<HTMLDivElement>();
	let status = $state<'starting' | 'running' | 'exited'>('starting');
	let failed = $state(false);

	const title = $derived(plugin.description || plugin.name);
	const subject = $derived.by(() => {
		const t = run.target ?? run.release;
		if (!t) return null;
		return t.namespace ? `${t.namespace}/${t.name}` : t.name;
	});

	$effect(() => {
		if (!element) return;
		return mountTerminal(element, (term) => {
			term.write(`\x1b[2mStarting ${plugin.name}…\x1b[0m\r\n`);
			return backend.openPlugin({ ...run, cols: term.cols, rows: term.rows }, (event) => {
				switch (event.type) {
					case 'ready':
						status = 'running';
						break;
					case 'data':
						term.write(event.data);
						break;
					case 'exit':
						status = 'exited';
						failed = event.message !== null;
						term.write(`\r\n\x1b[2m[${event.message ?? 'Done'}. Press Enter or Esc to close.]\x1b[0m\r\n`);
						break;
				}
			});
		});
	});

	function close() {
		app.pluginTerminal = null;
	}

	// While it runs, every key belongs to the plugin, Esc too. The terminal stops the keys it
	// handles from bubbling, so this listens before it.
	function onkeydown(e: KeyboardEvent) {
		if (status === 'exited' && (e.key === 'Enter' || e.key === 'Escape')) {
			e.preventDefault();
			e.stopPropagation();
			close();
		}
	}
</script>

<svelte:window onkeydowncapture={onkeydown} />

<div class="backdrop" role="presentation"></div>
<div class="window" role="dialog" aria-modal="true" aria-label={title}>
	<header>
		<div class="title">
			<strong>{title}</strong>
			{#if subject}<code>{subject}</code>{/if}
		</div>
		<span class="status" class:running={status === 'running'} class:failed>
			{status === 'running' ? 'Running' : status === 'exited' ? (failed ? 'Failed' : 'Done') : 'Starting…'}
		</span>
		<button onclick={close} title={status === 'exited' ? 'Close (Enter)' : 'Stop the plugin and close'}>
			{status === 'exited' ? 'Close' : 'Stop'}
		</button>
	</header>
	<div class="screen" style:background={TERMINAL_BACKGROUND}><div class="host" bind:this={element}></div></div>
</div>

<style>
	.backdrop {
		position: fixed;
		inset: 0;
		z-index: 45;
		background: rgb(0 0 0 / 40%);
	}
	.window {
		position: fixed;
		inset: 32px;
		z-index: 46;
		display: flex;
		flex-direction: column;
		border: 1px solid var(--border);
		border-radius: 12px;
		background: var(--surface-1);
		box-shadow: var(--shadow);
		overflow: hidden;
	}
	header {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 10px 16px;
		border-bottom: 1px solid var(--border);
	}
	.title {
		display: flex;
		align-items: baseline;
		gap: 10px;
		min-width: 0;
		flex: 1;
	}
	.title strong {
		white-space: nowrap;
	}
	code {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		color: var(--text-muted);
		font-family: var(--mono);
		font-size: 12.5px;
	}
	.status {
		font-size: 12.5px;
		color: var(--text-muted);
	}
	.status.running {
		color: var(--ok);
	}
	.status.failed {
		color: var(--bad);
	}
	button {
		padding: 4px 12px;
		border: 1px solid var(--border);
		border-radius: 6px;
		background: var(--surface-2);
		font-size: 12.5px;
	}
	.screen {
		flex: 1;
		min-height: 0;
		padding: 8px 0 8px 12px;
	}
	/* xterm sizes itself to this element, which must not have padding. */
	.host {
		height: 100%;
	}
</style>
