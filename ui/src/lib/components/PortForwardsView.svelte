<script lang="ts">
	import { forControl } from '../keys';
	import { tick } from 'svelte';
	import { stopPortForward } from '../actions';
	import { backend, type PortForwardInfo } from '../api';
	import { age, errorMessage } from '../format';
	import { app, favoriteLabel, isForwardOf } from '../state.svelte';
	import type { FavoriteForward } from '../api';
	import PageHeader from './PageHeader.svelte';

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
	/** The connected context's favorites that are not running. */
	const stopped = $derived(
		(app.cluster ? app.contextSettingsOf(app.cluster.context).forwards ?? [] : []).filter(
			(f) => !forwards.some((r) => r.context === app.cluster?.context && isForwardOf(f, r))
		)
	);
	let starting = $state<FavoriteForward | null>(null);

	async function start(f: FavoriteForward) {
		starting = f;
		try {
			await backend.startPortForward({
				target: { ...f.target, context: app.cluster?.context },
				remotePort: f.remotePort,
				localPort: f.localPort,
				address: f.address || null
			});
			await app.refreshForwards();
		} catch (err) {
			app.toast(`${favoriteLabel(f)} did not start: ${errorMessage(err)}`, 'bad');
		} finally {
			starting = null;
		}
	}
	const cursor = $derived.by(() => {
		const i = forwards.findIndex((f) => f.id === cursorId);
		return i === -1 ? 0 : i;
	});
	const current = $derived<PortForwardInfo | undefined>(forwards[cursor]);

	$effect(() => {
		void cursorId;
		tick().then(() => scroller?.querySelector('tr.cursor')?.scrollIntoView({ block: 'nearest' }));
	});

	/** A forward starts from an object: take the person to the pods. */
	function choose() {
		const pods = app.resources.find((r) => r.group === '' && r.plural === 'pods');
		if (pods) app.open(pods);
		app.toast('Select a pod and press Shift+F, or use its Port-forward action');
	}

	function url(f: PortForwardInfo) {
		// Listening on every address: in the browser, the server's own name reaches it.
		const everywhere = f.address === '0.0.0.0' || f.address === '::';
		const host = everywhere ? (backend.desktop ? 'localhost' : location.hostname) : f.address;
		return `http://${host.includes(':') && !host.startsWith('[') ? `[${host}]` : host}:${f.localPort}`;
	}

	function open(f: PortForwardInfo, e?: MouseEvent) {
		// The desktop window does not open links itself.
		if (e && !backend.desktop) return;
		e?.preventDefault();
		backend.openUrl(url(f)).catch((err) => app.toast(errorMessage(err), 'bad'));
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
		if (forControl(e)) return;
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
				case 'o':
					if (current) open(current);
					return true;
				case 'b':
					if (current) app.dialog = { kind: 'benchmark', forward: current };
					return true;
				case 's':
					if (current) app.toggleFavorite(current);
					return true;
			}
			return false;
		})();
		if (handled) e.preventDefault();
	}
</script>

<svelte:window {onkeydown} />

<section>
	<PageHeader
		crumbs={['Port-forwards']}
		title="Port-forwards"
		count={forwards.length}
		subtitle="Local ports tunneled to pods. They keep running while you work elsewhere in Kubri, and move to another pod when theirs goes away."
	/>

	<div class="card">
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
							{@const favorite = !!app.favoriteOf(f)}
							<tr class:cursor={i === cursor} onclick={() => (cursorId = f.id)}>
								<td class="mono">
									<a href={url(f)} target="_blank" rel="noreferrer" onclick={(e) => open(f, e)} title="Open in the browser (o)">{f.address}:{f.localPort}</a>
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
								<td>
									<div class="buttons">
										<button
											class="btn small star"
											class:on={favorite}
											aria-pressed={favorite}
											onclick={() => app.toggleFavorite(f)}
											title={favorite ? 'A favorite: starts whenever Kubri connects to this context (s)' : 'Keep as a favorite that starts with Kubri (s)'}
										>
											{favorite ? '★' : '☆'} Favorite
										</button>
										<button class="btn small" onclick={() => (app.dialog = { kind: 'benchmark', forward: f })} title="HTTP benchmark (b)">Benchmark</button>
										<button class="btn small danger" onclick={() => stopPortForward(f.id)} title="Stop (Ctrl+D)">Stop</button>
									</div>
								</td>
							</tr>
						{/each}
					</tbody>
				</table>
			{:else}
				<div class="empty" role="status">
					<p>No port-forwards running.</p>
					<p class="muted">Select a pod, deployment or service and press <kbd>Shift+F</kbd> to start one.</p>
					<button class="btn" onclick={choose}>Start a port-forward…</button>
				</div>
			{/if}
			{#if stopped.length}
				<div class="favorites">
					<h3>Favorites not running in {app.cluster ? app.contextName(app.cluster.context) : ''}</h3>
					<ul>
						{#each stopped as f (favoriteLabel(f))}
							<li>
								<span class="mono">{f.address || '127.0.0.1'}:{f.localPort}</span>
								<span class="mono muted">→ {f.target.resource.plural}/{f.target.name}:{f.remotePort}{f.target.namespace ? ` · ${f.target.namespace}` : ''}</span>
								<span class="buttons">
									<button class="btn small" onclick={() => start(f)} disabled={starting === f}>{starting === f ? 'Starting…' : 'Start'}</button>
									<button class="btn small" onclick={() => app.removeFavorite(f)} title="Forget this favorite">Remove</button>
								</span>
							</li>
						{/each}
					</ul>
				</div>
			{/if}
		</div>
	</div>
</section>

<style>
	section {
		display: flex;
		flex-direction: column;
		min-height: 0;
		min-width: 0;
		padding: 0 24px 20px;
	}
	.card {
		display: flex;
		flex-direction: column;
		flex: 1;
		min-height: 0;
		border: 1px solid var(--border);
		border-radius: 10px;
		background: var(--surface-1);
		overflow: hidden;
	}
	.favorites {
		padding: 8px 16px 16px;
		border-top: 1px solid var(--border-subtle);
	}
	.favorites h3 {
		margin: 8px 0;
		font-size: 11px;
		font-weight: 600;
		letter-spacing: 0.06em;
		text-transform: uppercase;
		color: var(--text-muted);
	}
	.favorites ul {
		display: grid;
		gap: 6px;
		margin: 0;
		padding: 0;
		list-style: none;
		font-size: 13px;
	}
	.favorites li {
		display: flex;
		align-items: center;
		gap: 12px;
	}
	.favorites .buttons {
		margin-left: auto;
	}
	.star.on {
		color: var(--accent-strong);
	}
	.table-wrap {
		flex: 1;
		min-height: 0;
		overflow: auto;
	}
	table {
		width: 100%;
		border-collapse: collapse;
		font-size: 13px;
	}
	th {
		position: sticky;
		top: 0;
		z-index: 1;
		padding: 9px 10px;
		background: var(--surface-1);
		border-bottom: 1px solid var(--border);
		text-align: left;
		font-size: 12px;
		font-weight: 600;
		color: var(--text-muted);
		white-space: nowrap;
	}
	th:first-child,
	td:first-child {
		padding-left: 16px;
	}
	td {
		padding: 11px 10px;
		border-bottom: 1px solid var(--border-subtle);
		white-space: nowrap;
		vertical-align: top;
	}
	tbody tr:hover {
		background: color-mix(in srgb, var(--surface-2) 55%, transparent);
	}
	tbody tr.cursor {
		background: color-mix(in srgb, var(--accent) 10%, var(--surface-1));
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
	.buttons {
		display: flex;
		gap: 6px;
		justify-content: flex-end;
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
	@media (max-width: 720px) {
		section {
			padding: 0 12px 12px;
		}
	}
</style>
