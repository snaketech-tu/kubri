<script lang="ts">
	import { tick } from 'svelte';
	import { stopPortForward } from '../actions';
	import type { PortForwardInfo } from '../api';
	import { age } from '../format';
	import { app } from '../state.svelte';

	let cursorId = $state<number | null>(null);
	let now = $state(Date.now());
	let scroller = $state<HTMLDivElement>();

	$effect(() => {
		app.refreshForwards();
		const timer = setInterval(() => {
			now = Date.now();
			app.refreshForwards();
		}, 2000);
		return () => clearInterval(timer);
	});

	const forwards = $derived(app.forwards);
	const cursor = $derived.by(() => {
		const i = forwards.findIndex((f) => f.id === cursorId);
		return i === -1 ? 0 : i;
	});
	const current = $derived<PortForwardInfo | undefined>(forwards[cursor]);

	$effect(() => {
		void cursorId;
		tick().then(() => scroller?.querySelector('tr.cursor')?.scrollIntoView({ block: 'nearest' }));
	});

	function url(f: PortForwardInfo) {
		const host = f.address === '0.0.0.0' || f.address === '::' ? 'localhost' : f.address;
		return `http://${host.includes(':') ? `[${host}]` : host}:${f.localPort}`;
	}

	function move(delta: number) {
		if (!forwards.length) return;
		cursorId = forwards[Math.max(0, Math.min(forwards.length - 1, cursor + delta))].id;
	}

	function copy(f: PortForwardInfo) {
		navigator.clipboard.writeText(url(f)).then(() => app.toast(`Copied ${url(f)}`));
	}

	function onkeydown(e: KeyboardEvent) {
		if (app.overlayOpen) return;
		if ((e.target as HTMLElement).closest('input, textarea, select, [contenteditable="true"]')) return;
		const ctrl = e.ctrlKey || e.metaKey;
		const handled = (() => {
			if (ctrl && e.key.toLowerCase() === 'd') {
				if (current) stopPortForward(current.id);
				return true;
			}
			if (ctrl || e.altKey) return false;
			switch (e.key) {
				case 'ArrowDown':
				case 'j':
					move(1);
					return true;
				case 'ArrowUp':
				case 'k':
					move(-1);
					return true;
				case 'Delete':
					if (current) stopPortForward(current.id);
					return true;
				case 'Enter':
				case 'c':
					if (current) copy(current);
					return true;
			}
			return false;
		})();
		if (handled) e.preventDefault();
	}
</script>

<svelte:window {onkeydown} />

<section>
	<header>
		<h1>Port-forwards</h1>
		<span class="count">{forwards.length}</span>
	</header>

	<div class="table-wrap" bind:this={scroller}>
		{#if forwards.length}
			<table>
				<thead>
					<tr>
						<th>Local</th>
						<th>Target</th>
						<th>Pod</th>
						<th>Context</th>
						<th class="numeric">Conns</th>
						<th>Age</th>
						<th></th>
					</tr>
				</thead>
				<tbody>
					{#each forwards as f, i (f.id)}
						<tr class:cursor={i === cursor} onclick={() => (cursorId = f.id)}>
							<td class="mono">
								<a href={url(f)} target="_blank" rel="noreferrer">{f.address}:{f.localPort}</a>
								<button class="link" onclick={() => copy(f)} title="Copy URL (c)">copy</button>
							</td>
							<td class="mono">{f.target.resource.plural}/{f.target.name}{f.target.namespace ? ` · ${f.target.namespace}` : ''}</td>
							<td class="mono">
								{f.pod}:{f.podPort}
								{#if f.error}<div class="error" title={f.error}>{f.error}</div>{/if}
							</td>
							<td class="muted">{f.context}</td>
							<td class="numeric">{f.connections}</td>
							<td class="muted">{age(f.started, now)}</td>
							<td><button class="stop" onclick={() => stopPortForward(f.id)} title="Stop (Ctrl+D)">Stop</button></td>
						</tr>
					{/each}
				</tbody>
			</table>
		{:else}
			<div class="empty">
				<p>No port-forwards running.</p>
				<p class="muted">Select a pod, deployment or service and press <kbd>Shift+F</kbd> to start one.</p>
			</div>
		{/if}
	</div>
</section>

<style>
	section {
		display: flex;
		flex-direction: column;
		min-height: 0;
		min-width: 0;
	}
	header {
		display: flex;
		align-items: baseline;
		gap: 10px;
		padding: 14px 20px 10px;
	}
	h1 {
		margin: 0;
		font-size: 20px;
	}
	.count {
		padding: 1px 8px;
		border-radius: 999px;
		background: var(--surface-2);
		font-size: 12px;
		font-weight: 600;
	}
	.table-wrap {
		flex: 1;
		overflow: auto;
		padding: 0 12px 16px;
	}
	table {
		width: 100%;
		border-collapse: collapse;
		font-size: 13px;
	}
	th {
		position: sticky;
		top: 0;
		padding: 8px 10px;
		background: var(--bg);
		border-bottom: 1px solid var(--border);
		text-align: left;
		font-size: 11px;
		font-weight: 600;
		letter-spacing: 0.05em;
		text-transform: uppercase;
		color: var(--text-muted);
	}
	td {
		padding: 6px 10px;
		border-bottom: 1px solid var(--border-subtle);
		white-space: nowrap;
		vertical-align: top;
	}
	tbody tr:hover {
		background: var(--surface-1);
	}
	tbody tr.cursor {
		background: color-mix(in srgb, var(--accent) 16%, transparent);
		box-shadow: inset 3px 0 0 var(--accent);
	}
	.mono {
		font-family: var(--mono);
		font-size: 12.5px;
	}
	a {
		color: var(--accent-strong);
	}
	.link {
		margin-left: 6px;
		padding: 0;
		border: none;
		background: none;
		color: var(--text-muted);
		font-size: 11px;
		text-decoration: underline;
	}
	.error {
		max-width: 360px;
		overflow: hidden;
		text-overflow: ellipsis;
		color: var(--bad);
		font-size: 11.5px;
	}
	.numeric {
		text-align: right;
		font-variant-numeric: tabular-nums;
	}
	.muted {
		color: var(--text-muted);
	}
	.stop {
		padding: 2px 10px;
		border: 1px solid var(--border);
		border-radius: 6px;
		background: var(--surface-1);
		font-size: 12px;
	}
	.stop:hover {
		border-color: var(--bad);
		color: var(--bad);
	}
	.empty {
		padding: 40px 24px;
		text-align: center;
	}
	.empty p {
		margin: 4px 0;
	}
	kbd {
		padding: 0 5px;
		border: 1px solid var(--border);
		border-radius: 4px;
		background: var(--surface-2);
		font-family: var(--mono);
		font-size: 11px;
	}
</style>
