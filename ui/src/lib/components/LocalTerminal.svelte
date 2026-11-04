<script lang="ts">
	import { untrack } from 'svelte';
	import { backend, type LocalTerminalInfo } from '../api';
	import type { LocalSession } from '../dock.svelte';
	import { errorMessage } from '../format';
	import { app } from '../state.svelte';
	import { mountTerminal, TERMINAL_BACKGROUND } from '../terminal';

	let { session }: { session: LocalSession } = $props();
	const s = untrack(() => session);

	const INSTALL = 'https://kubernetes.io/docs/tasks/tools/';

	let info = $state<LocalTerminalInfo | null>(null);
	let status = $state<'checking' | 'refused' | 'starting' | 'open' | 'exited'>('checking');
	let restarts = $state(0);
	let element = $state<HTMLDivElement>();

	/** The core opens terminals for the connected context only. */
	const here = $derived(app.cluster?.context === s.context && !app.snapshot);
	const where = $derived(s.namespace ? `${app.contextName(s.context)}/${s.namespace}` : app.contextName(s.context));
	const production = $derived(app.contextSettingsOf(s.context).production);

	$effect(() => {
		void restarts;
		if (!element) return;
		const host = element;
		let cancelled = false;
		let unmount: (() => void) | null = null;
		status = 'checking';
		untrack(() => backend.localTerminalInfo()).then(
			(found) => {
				if (cancelled) return;
				info = found;
				if (!found.allowed || found.context !== s.context) {
					status = 'refused';
					return;
				}
				const { fontSize, fontFamily } = untrack(() => app.settings.terminal ?? { fontSize: 13, fontFamily: '' });
				unmount = mountTerminal(
					host,
					(term) => {
						status = 'starting';
						return backend.openLocalTerminal({ namespace: s.namespace, cols: term.cols, rows: term.rows }, (event) => {
							switch (event.type) {
								case 'ready':
									status = 'open';
									break;
								case 'data':
									term.write(event.data);
									break;
								case 'exit':
									status = 'exited';
									term.write(`\r\n\x1b[2m[${event.message ?? 'Session ended'}]\x1b[0m\r\n`);
									break;
							}
						});
					},
					{ fontSize, fontFamily }
				);
			},
			(err) => {
				if (cancelled) return;
				info = null;
				status = 'refused';
				app.toast(`Could not open a terminal: ${errorMessage(err)}`, 'bad');
			}
		);
		return () => {
			cancelled = true;
			unmount?.();
			host.replaceChildren();
		};
	});

	async function unlock() {
		await app.unlock();
		if (!app.guard.readOnly) restarts++;
	}

	const refusal = $derived.by(() => {
		if (status !== 'refused') return null;
		if (!here) return `This terminal belongs to ${app.contextName(s.context)}. Switch to it to open the terminal again.`;
		return info?.reason ?? 'Kubri could not tell whether a terminal may be opened here.';
	});
</script>

<div class="terminal" style:--terminal-bg={TERMINAL_BACKGROUND}>
	<div class="toolbar">
		<strong>Terminal</strong>
		<code
			class:production
			title={s.namespace
				? 'kubectl and helm here talk to this context and namespace'
				: "kubectl and helm here talk to this context, in the context's own namespace"}>{where}</code
		>
		{#if info?.allowed}
			<span class="tool" title={info.shell}>
				{#if info.kubectl}kubectl {info.kubectl}{:else}<a href={INSTALL} target="_blank" rel="noreferrer">kubectl not found</a>{/if}
			</span>
			{#if info.helm}<span class="tool">helm {info.helm}</span>{/if}
			{#if info.skew}<a class="skew" href="{INSTALL}" target="_blank" rel="noreferrer" title={info.skew}>kubectl version too far from the cluster's</a>{/if}
		{/if}
		<span class="status" class:open={status === 'open'} class:exited={status === 'exited' || status === 'refused'}>
			{status === 'open'
				? 'Running'
				: status === 'exited'
					? 'Ended'
					: status === 'refused'
						? 'Not opened'
						: 'Starting…'}
		</span>
		<span class="spacer"></span>
		<button onclick={() => restarts++} disabled={!here} title={here ? '' : `Switch to ${app.contextName(s.context)} to open it again`}>
			{status === 'open' || status === 'starting' ? 'Restart' : 'Open again'}
		</button>
	</div>
	{#if status === 'open' && here && app.guard.readOnly}
		<p class="note" role="status">Changes are off for this context again. This shell keeps running, without Kubri's checks.</p>
	{/if}
	{#if refusal}
		<div class="refused" role="status">
			<p>{refusal}</p>
			{#if here && app.guard.readOnly && app.guard.canUnlock}
				<button onclick={unlock}>Unlock for 15 minutes</button>
			{/if}
		</div>
	{/if}
	<div class="screen" hidden={status === 'refused'}><div class="host" bind:this={element}></div></div>
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
		white-space: nowrap;
		overflow: hidden;
	}
	code {
		font-family: var(--mono);
		color: var(--text-muted);
		overflow: hidden;
		text-overflow: ellipsis;
	}
	code.production {
		color: var(--bad);
	}
	.tool {
		color: var(--text-muted);
	}
	.skew {
		color: var(--warn);
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
	.note {
		margin: 0;
		padding: 6px 16px;
		border-bottom: 1px solid var(--border-subtle);
		color: var(--warn);
		font-size: 12.5px;
	}
	.refused {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 10px;
		padding: 16px;
		font-size: 13px;
	}
	.refused p {
		margin: 0;
		max-width: 70ch;
	}
	.screen {
		flex: 1;
		min-height: 0;
		overflow: hidden;
		padding: 6px 0 6px 10px;
		background: var(--terminal-bg);
	}
	.screen[hidden] {
		display: none;
	}
	/* xterm sizes itself to this element, which must not have padding. */
	.host {
		height: 100%;
	}
</style>
