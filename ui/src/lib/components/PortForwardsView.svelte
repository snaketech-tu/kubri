<script lang="ts">
	import { t } from '../i18n';
	import { forControl } from '../keys';
	import { follow } from '../roving';
	import { tick } from 'svelte';
	import { stopPortForward } from '../actions';
	import { backend, type PortForwardInfo } from '../api';
	import { age, errorMessage } from '../format';
	import { lastEnd } from '../forwards';
	import { app, favoriteLabel } from '../state.svelte';
	import PageHeader from './PageHeader.svelte';
	import Phrase from './Phrase.svelte';

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
		tick().then(() => {
			const row = scroller?.querySelector<HTMLElement>('tr.cursor');
			follow(row, row?.parentElement);
		});
	});

	/** A snapshot has no cluster to forward to. */
	const canStart = $derived(!!app.cluster && !app.cluster.snapshot);

	/** Asks for the namespace, the object and the port. */
	function choose() {
		if (canStart) app.dialog = { kind: 'newForward' };
	}

	function url(f: PortForwardInfo) {
		// Listening on every address: in the browser, the server's own name reaches it.
		const everywhere = f.address === '0.0.0.0' || f.address === '::';
		const host = everywhere ? (backend.desktop ? 'localhost' : location.hostname) : f.address;
		return `http://${host.includes(':') && !host.startsWith('[') ? `[${host}]` : host}:${f.localPort}`;
	}

	function open(f: PortForwardInfo) {
		backend.openUrl(url(f)).catch((err) => app.toast(errorMessage(err), 'bad'));
	}

	function move(delta: number) {
		if (!forwards.length) return;
		cursorId = forwards[Math.max(0, Math.min(forwards.length - 1, cursor + delta))].id;
	}

	function copy(f: PortForwardInfo) {
		navigator.clipboard.writeText(url(f)).then(() => app.toast(t('Copied {url}', { url: url(f) })));
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
					if (current) app.favorites.toggle(current);
					return true;
				case 'F':
					choose();
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
		crumbs={[t('Port-forwards')]}
		title={t('Port-forwards')}
		count={forwards.length}
		subtitle={t('Local ports tunneled to pods. They keep running while you work elsewhere in Kubri, and move to another pod when theirs goes away.')}
	>
		{#snippet tools()}
			{#if canStart && forwards.length}<button class="btn" onclick={choose} title={t('Choose a namespace, a target and a port (Shift+F)')}>{t('Start a port-forward…')}</button>{/if}
		{/snippet}
	</PageHeader>

	<div class="card">
		<div class="table-wrap" bind:this={scroller}>
			{#if forwards.length}
				<table>
					<thead>
						<tr>
							<th>{t('Local')}</th>
							<th>{t('Target')}</th>
							<th>{t('Pod')}</th>
							<th>{t('Context')}</th>
							<th class="numeric">{t('Conns')}</th>
							<th>{t('Age')}</th>
							<th></th>
						</tr>
					</thead>
					<tbody>
						{#each forwards as f, i (f.id)}
							{@const favorite = !!app.favorites.of(f)}
							<tr class:cursor={i === cursor} onclick={() => (cursorId = f.id)} onfocusin={() => (cursorId = f.id)}>
								<td class="mono">
									<a href={url(f)} target="_blank" rel="noreferrer" title={t('Open in the browser (o)')}>{f.address}:{f.localPort}</a>
									<button class="link" onclick={() => copy(f)} title={t('Copy URL (c)')}>{t('copy')}</button>
								</td>
								<td class="mono">{f.target.resource.plural}/{f.target.name}{f.target.namespace ? ` · ${f.target.namespace}` : ''}</td>
								<td class="mono">
									{f.pod}:{f.podPort}
									{#if f.error}<div class="error" title={f.error}>{f.error}</div>
									{:else if lastEnd(f, now)}<div class="ended">{lastEnd(f, now)}</div>{/if}
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
											onclick={() => app.favorites.toggle(f)}
											title={favorite ? t('A favorite: start it again with one click from the sidebar (s)') : t('Keep as a favorite to start with one click (s)')}
										>
											{favorite ? '★' : '☆'} {t('Favorite')}
										</button>
										<button class="btn small" onclick={() => (app.dialog = { kind: 'benchmark', forward: f })} title={t('HTTP benchmark (b)')}>{t('Benchmark')}</button>
										<button class="btn small danger" onclick={() => stopPortForward(f.id)} title={t('Stop (Ctrl+D)')}>{t('Stop')}</button>
									</div>
								</td>
							</tr>
						{/each}
					</tbody>
				</table>
			{:else}
				<div class="empty" role="status">
					<p>{t('No port-forwards running.')}</p>
					<p class="muted"><Phrase text={t('Select a pod, deployment or service and press {key} to start one.')} kbd={{ key: 'Shift+F' }} /></p>
					{#if canStart}<button class="btn" onclick={choose} title={t('Choose a namespace, a target and a port (Shift+F)')}>{t('Start a port-forward…')}</button>{/if}
				</div>
			{/if}
			{#if app.favorites.here.length}
				<div class="favorites">
					<h3>{t('Favorites in {context}', { context: app.cluster ? app.prefs.contextName(app.cluster.context) : '' })}</h3>
					<ul>
						{#each app.favorites.here as f (favoriteLabel(f))}
							{@const on = !!app.favorites.running(f)}
							{@const busy = app.favorites.switching.includes(f)}
							{@const running = app.favorites.running(f)}
							<li class:on>
								<span class="dot" title={on ? t('Running') : t('Stopped')}></span>
								<div class="what">
									<div class="name">
										<strong>{f.target.name}</strong>
										<span class="muted">{f.target.resource.plural}{f.target.namespace ? ` · ${f.target.namespace}` : ''}</span>
									</div>
									<div class="mono route">
										{#if running}
											<a href={url(running)} target="_blank" rel="noreferrer" title={t('Open in the browser (o)')}>{f.address || '127.0.0.1'}:{f.localPort}</a>
										{:else}
											{f.address || '127.0.0.1'}:{f.localPort}
										{/if}
										<span class="muted">→ {f.remotePort}</span>
									</div>
								</div>
								<span class="buttons">
									<label class="check" title={t('Start it whenever Kubri connects to this context')}>
										<input type="checkbox" checked={f.startOnConnect} onchange={(e) => app.favorites.setStartOnConnect(f, e.currentTarget.checked)} />
										{t('Start on connect')}
									</label>
									{#if running}
										<button class="btn" onclick={() => open(running)}>{t('Open')}</button>
									{/if}
									<button class="btn" class:danger={on} onclick={() => app.favorites.switch(f)} disabled={busy}>
										{busy ? (on ? t('Stopping…') : t('Starting…')) : on ? t('Stop') : t('Start')}
									</button>
									<button class="btn" onclick={() => app.favorites.remove(f)} title={t('Forget this favorite')}>{t('Remove')}</button>
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
		padding: 12px 16px 18px;
		border-top: 1px solid var(--border-subtle);
	}
	.favorites h3 {
		margin: 6px 0 10px;
		font-size: 11px;
		font-weight: 600;
		letter-spacing: 0.06em;
		text-transform: uppercase;
		color: var(--text-muted);
	}
	.favorites ul {
		display: grid;
		gap: 8px;
		margin: 0;
		padding: 0;
		list-style: none;
		font-size: 14px;
	}
	.favorites li {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 10px 14px;
		padding: 12px 14px;
		border: 1px solid var(--border-subtle);
		border-radius: 8px;
		background: var(--surface-1);
	}
	.favorites li.on {
		border-color: color-mix(in srgb, var(--ok) 35%, var(--border-subtle));
		background: color-mix(in srgb, var(--ok) 5%, var(--surface-1));
	}
	.favorites .what {
		display: grid;
		gap: 3px;
		min-width: 0;
	}
	.favorites .name {
		display: flex;
		align-items: baseline;
		gap: 8px;
		min-width: 0;
	}
	.favorites .name strong {
		font-size: 15px;
		font-weight: 600;
	}
	.favorites .name .muted {
		font-size: 12.5px;
	}
	.favorites .route {
		font-size: 13px;
	}
	.favorites .buttons {
		display: flex;
		align-items: center;
		gap: 8px;
		margin-left: auto;
	}
	.favorites .dot {
		flex: none;
		width: 10px;
		height: 10px;
		border: 2px solid var(--text-muted);
		border-radius: 50%;
	}
	.favorites li.on .dot {
		border-color: var(--ok);
		background: var(--ok);
	}
	.check {
		display: flex;
		align-items: center;
		gap: 6px;
		color: var(--text-muted);
		font-size: 12px;
		white-space: nowrap;
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
		font-size: 14px;
	}
	th {
		position: sticky;
		top: 0;
		z-index: 1;
		padding: 11px 12px;
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
		padding: 15px 12px;
		border-bottom: 1px solid var(--border-subtle);
		white-space: nowrap;
		vertical-align: middle;
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
		font-size: 13px;
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
	.ended {
		color: var(--text-muted);
		font-family: var(--sans);
		font-size: 11.5px;
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
	.empty :global(kbd) {
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
