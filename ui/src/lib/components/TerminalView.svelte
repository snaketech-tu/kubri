<script lang="ts">
	import { t } from '../i18n';
	import { untrack } from 'svelte';
	import { backend, type ObjectRef, type TerminalMode } from '../api';
	import { mountTerminal, TERMINAL_BACKGROUND } from '../terminal';
	import { plainOptions } from '../select';
	import Select from './Select.svelte';

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
		// Read here, so that a change opens a new terminal: the terminal connects once its fonts are in.
		const [to, wantedContainer, wantedMode] = [target, container, mode];
		return mountTerminal(element, (term) => {
			status = 'connecting';
			const starting = wantedMode === 'shell' ? t('Starting a shell in {name}…', { name: to.name }) : t('Attaching to {name}…', { name: to.name });
			term.write(`\x1b[2m${starting}\x1b[0m\r\n`);
			return backend.openTerminal(
				to,
				{ container: wantedContainer || null, mode: wantedMode, cols: term.cols, rows: term.rows },
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
			<Select bind:value={container} options={[{ value: '', label: t('Default container') }, ...plainOptions(containers)]} title={t('Container')} compact />
		{/if}
		<Select
			bind:value={mode}
			options={[
				{ value: 'shell', label: t('Shell') },
				{ value: 'attach', label: t('Attach') }
			]}
			title={t('Run a new shell, or connect to the container\'s main process')}
			compact
		/>
		<span class="status" class:open={status === 'open'} class:exited={status === 'exited'}>
			{status === 'open' ? t('Connected to {name}', { name: connectedTo }) : status === 'exited' ? t('Ended') : t('Connecting…')}
		</span>
		<span class="spacer"></span>
		<button onclick={() => restarts++}>{status === 'exited' ? t('Reconnect') : t('Restart')}</button>
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
