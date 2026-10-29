<script lang="ts">
	import { untrack } from 'svelte';
	import { backend, type ObjectRef, type TerminalMode } from '../api';
	import { mountTerminal, TERMINAL_BACKGROUND } from '../terminal';

	let {
		target,
		containers,
		mode: startMode = 'shell',
		container: wanted = null
	}: {
		target: ObjectRef;
		containers: string[];
		mode?: TerminalMode;
		/** A container to connect to, asked for from Describe; a new object each time. */
		container?: { name: string; mode?: TerminalMode } | null;
	} = $props();

	let container = $state(untrack(() => wanted?.name ?? ''));
	let mode = $state<TerminalMode>(untrack(() => wanted?.mode ?? startMode));
	$effect(() => {
		if (!wanted) return;
		container = wanted.name;
		if (wanted.mode) mode = wanted.mode;
	});
	let status = $state<'connecting' | 'open' | 'exited'>('connecting');
	let connectedTo = $state('');
	let restarts = $state(0);
	let element = $state<HTMLDivElement>();

	$effect(() => {
		void restarts;
		if (!element) return;
		return mountTerminal(element, (term) => {
			status = 'connecting';
			term.write(`\x1b[2m${mode === 'shell' ? 'Starting a shell' : 'Attaching'} in ${target.name}…\x1b[0m\r\n`);
			return backend.openTerminal(
				target,
				{ container: container || null, mode, cols: term.cols, rows: term.rows },
				(event) => {
					switch (event.type) {
						case 'ready':
							status = 'open';
							connectedTo = event.container;
							break;
						case 'data':
							term.write(event.data);
							break;
						case 'exit':
							status = 'exited';
							term.write(`\r\n\x1b[2m[${event.message ?? 'Session ended'}]\x1b[0m\r\n`);
							break;
					}
				}
			);
		});
	});
</script>

<div class="terminal" style:--terminal-bg={TERMINAL_BACKGROUND}>
	<div class="toolbar">
		{#if containers.length > 1}
			<select bind:value={container} title="Container">
				<option value="">Default container</option>
				{#each containers as c (c)}<option value={c}>{c}</option>{/each}
			</select>
		{/if}
		<select bind:value={mode} title="Run a new shell, or connect to the container's main process">
			<option value="shell">Shell</option>
			<option value="attach">Attach</option>
		</select>
		<span class="status" class:open={status === 'open'} class:exited={status === 'exited'}>
			{status === 'open' ? `Connected to ${connectedTo}` : status === 'exited' ? 'Ended' : 'Connecting…'}
		</span>
		<span class="spacer"></span>
		<button onclick={() => restarts++}>{status === 'exited' ? 'Reconnect' : 'Restart'}</button>
	</div>
	<div class="screen"><div class="host" bind:this={element}></div></div>
</div>

<style>
	.terminal {
		display: flex;
		flex-direction: column;
		min-height: 0;
		overflow: hidden;
	}
	.toolbar {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 8px 16px;
		border-bottom: 1px solid var(--border-subtle);
		font-size: 12.5px;
	}
	select {
		padding: 3px 8px;
		font-size: 12.5px;
	}
	.status {
		color: var(--text-muted);
	}
	.status.open {
		color: var(--ok);
	}
	.status.exited {
		color: var(--warn);
	}
	.spacer {
		flex: 1;
	}
	button {
		padding: 3px 10px;
		border: 1px solid var(--border);
		border-radius: 6px;
		background: var(--surface-1);
		font-size: 12px;
	}
	.screen {
		flex: 1;
		min-height: 0;
		overflow: hidden;
		padding: 6px 0 6px 10px;
		background: var(--terminal-bg);
	}
	/* xterm sizes itself to this element, which must not have padding. */
	.host {
		height: 100%;
	}
</style>
