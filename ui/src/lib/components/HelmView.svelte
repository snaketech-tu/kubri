<script lang="ts">
	import { tick } from 'svelte';
	import { stringify } from 'yaml';
	import { backend, type HelmRelease, type HelmReleaseDetails } from '../api';
	import { age, errorMessage } from '../format';
	import {
		chartLabel,
		filterReleases,
		hasValues,
		manifestObjects,
		manifestResource,
		previousRevision,
		releaseKey,
		statusHealth,
		type ManifestObject
	} from '../helm';
	import { forControl } from '../keys';
	import type { MenuAction } from '../menu';
	import { app } from '../state.svelte';
	import HealthIcon from './HealthIcon.svelte';
	import YamlEditor from './YamlEditor.svelte';

	type Tab = 'overview' | 'values' | 'manifest' | 'history';
	const TABS: [Tab, string, string][] = [
		['overview', 'Overview', 'n'],
		['values', 'Values', 'v'],
		['manifest', 'Manifest', 'm'],
		['history', 'History', 'h']
	];
	const REFRESH_MS = 30_000;

	let releases = $state<HelmRelease[] | null>(null);
	let error = $state<string | null>(null);
	let helm = $state<string | null | undefined>(undefined);
	let filter = $state('');
	let cursorKey = $state<string | null>(null);
	let reloads = $state(0);
	let now = $state(Date.now());

	// The release being looked at, and which of its revisions; null is the latest.
	let opened = $state<{ namespace: string; name: string } | null>(null);
	let revision = $state<number | null>(null);
	let tab = $state<Tab>('overview');
	let allValues = $state(false);
	let details = $state<HelmReleaseDetails | null>(null);
	let history = $state<HelmRelease[]>([]);
	let detailsError = $state<string | null>(null);
	let historyCursor = $state(0);

	let scroller = $state<HTMLDivElement>();
	let filterInput = $state<HTMLInputElement>();

	const namespace = $derived(app.namespace || null);

	$effect(() => {
		backend.helmVersion().then(
			(v) => (helm = v),
			() => (helm = null)
		);
	});

	$effect(() => {
		void reloads;
		const ns = namespace;
		let cancelled = false;
		const load = () =>
			backend.helmReleases(ns).then(
				(r) => {
					if (cancelled) return;
					releases = r;
					error = null;
					now = Date.now();
				},
				(err) => !cancelled && (error = errorMessage(err))
			);
		load();
		const timer = setInterval(load, REFRESH_MS);
		return () => {
			cancelled = true;
			clearInterval(timer);
		};
	});

	$effect(() => {
		void reloads;
		const target = opened;
		const rev = revision;
		if (!target) return;
		let cancelled = false;
		detailsError = null;
		backend.helmRelease(target.namespace, target.name, rev ?? undefined).then(
			(d) => !cancelled && (details = d),
			(err) => !cancelled && (detailsError = errorMessage(err))
		);
		backend.helmHistory(target.namespace, target.name).then(
			(h) => !cancelled && (history = h),
			() => !cancelled && (history = [])
		);
		return () => (cancelled = true);
	});

	const visible = $derived(filterReleases(releases ?? [], filter));
	const cursor = $derived.by(() => {
		const i = visible.findIndex((r) => releaseKey(r) === cursorKey);
		return i === -1 ? 0 : i;
	});
	const current = $derived<HelmRelease | undefined>(visible[cursor]);
	/** The release as it is now, for actions, even while an older revision is shown. */
	const latest = $derived(
		opened ? (releases ?? []).find((r) => r.namespace === opened!.namespace && r.name === opened!.name) : undefined
	);
	const shown = $derived(details && opened && releaseKey(details) === releaseKey(opened) ? details : null);
	const objects = $derived(shown ? manifestObjects(shown.manifest) : []);
	const valuesText = $derived.by(() => {
		if (!shown) return '';
		const values = allValues ? shown.allValues : shown.values;
		return hasValues(values) ? stringify(values, { lineWidth: 0 }) : '';
	});

	// Plugins and hotkeys run on the opened release, or else the one under the cursor.
	$effect(() => {
		const focus = () => {
			const row = opened ? latest : current;
			const release = opened ?? (current ? { namespace: current.namespace, name: current.name } : null);
			const cell = (name: string, value: string | number) => ({ name, value: String(value) });
			const columns = row
				? [
						...(namespace ? [] : [cell('Namespace', row.namespace)]),
						cell('Name', row.name),
						cell('Revision', row.revision),
						cell('Status', row.status),
						cell('Chart', chartLabel(row)),
						cell('App version', row.appVersion),
						cell('Age', age(row.updated, Date.now()))
					]
				: [];
			return { resource: null, target: null, release, columns, filter };
		};
		app.focus = focus;
		return () => {
			if (app.focus === focus) app.focus = null;
		};
	});

	$effect(() => {
		void cursorKey;
		void historyCursor;
		tick().then(() => scroller?.querySelector('tr.cursor')?.scrollIntoView({ block: 'nearest' }));
	});

	function move(delta: number) {
		if (opened && tab === 'history') {
			historyCursor = Math.max(0, Math.min(history.length - 1, historyCursor + delta));
			return;
		}
		if (!visible.length) return;
		cursorKey = releaseKey(visible[Math.max(0, Math.min(visible.length - 1, cursor + delta))]);
	}

	function open(release: HelmRelease, to: Tab = 'overview') {
		cursorKey = releaseKey(release);
		if (!opened || releaseKey(opened) !== releaseKey(release)) {
			opened = { namespace: release.namespace, name: release.name };
			revision = null;
			details = null;
			history = [];
			historyCursor = 0;
		}
		tab = to;
	}

	/** A release's menu in the list, with the keys the list uses. */
	function openMenu(e: MouseEvent, release: HelmRelease) {
		e.preventDefault();
		cursorKey = releaseKey(release);
		const actions: MenuAction[] = TABS.map(([to, label, key]) => ({
			group: 'View',
			label: to === 'overview' ? 'Open' : label,
			keys: [key],
			hint: to === 'overview' ? 'enter' : undefined,
			run: () => open(release, to)
		}));
		if (helm) {
			actions.push(
				{ group: 'Change', label: 'Roll back…', keys: ['r'], run: () => void rollback(release) },
				{ group: 'Delete', label: 'Uninstall…', keys: ['ctrl+d'], danger: true, run: () => uninstall(release) }
			);
		}
		app.menu = { actions, x: e.clientX, y: e.clientY };
	}

	function back() {
		opened = null;
		revision = null;
		details = null;
		history = [];
		app.details = null;
		app.maximized = false;
		tick().then(() => scroller?.querySelector('tr.cursor')?.scrollIntoView({ block: 'nearest' }));
	}

	function viewRevision(rev: number) {
		const newest = history[0]?.revision;
		revision = rev === newest ? null : rev;
		tab = 'overview';
	}

	async function rollback(release: HelmRelease | undefined, to?: number) {
		if (!release || !helm) return;
		let target = to;
		if (target === undefined) {
			const known = opened && releaseKey(opened) === releaseKey(release) && history.length;
			const revisions = known ? history : await backend.helmHistory(release.namespace, release.name).catch(() => []);
			target = previousRevision(revisions, release.revision) ?? undefined;
		}
		if (!target || target === release.revision) {
			app.toast(`${release.name} has no earlier revision to roll back to`, 'bad');
			return;
		}
		app.dialog = { kind: 'helmRollback', release, revision: target, done: reload };
	}

	function uninstall(release: HelmRelease | undefined) {
		if (!release || !helm) return;
		app.dialog = {
			kind: 'helmUninstall',
			release,
			done: () => {
				if (opened && releaseKey(opened) === releaseKey(release)) back();
				reload();
			}
		};
	}

	function reload() {
		revision = null;
		reloads++;
	}

	function openObject(object: ManifestObject) {
		const resource = manifestResource(app.resources, object);
		if (!resource || !opened) return;
		const ns = resource.namespaced ? (object.namespace ?? opened.namespace) : null;
		app.showDetails(resource, { resource, namespace: ns, name: object.name });
	}

	/** Whether the open release can go back to revision `target`. */
	function canRollBack(target: number) {
		return !!helm && !!latest && target !== latest.revision;
	}

	function onkeydown(e: KeyboardEvent) {
		if (app.overlayOpen) return;
		if (e.target === filterInput) {
			if (['Escape', 'Enter', 'ArrowDown'].includes(e.key)) {
				if (e.key === 'Escape') filter = '';
				filterInput?.blur();
				e.preventDefault();
			}
			return;
		}
		if (forControl(e)) return;
		const ctrl = e.ctrlKey || e.metaKey;
		const handled = (() => {
			const release = opened ? latest : current;
			if ((ctrl && e.key.toLowerCase() === 'd') || (!ctrl && e.key === 'Delete')) {
				uninstall(release);
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
				case 'g':
					move(-Infinity);
					return true;
				case 'G':
					move(Infinity);
					return true;
				case 'Enter':
				case 'd':
					if (opened && tab === 'history') {
						if (history[historyCursor]) viewRevision(history[historyCursor].revision);
					} else if (!opened && current) {
						open(current);
					}
					return true;
				case 'n':
				case 'v':
				case 'm':
				case 'h': {
					const to = TABS.find(([, , key]) => key === e.key)![0];
					if (opened) tab = to;
					else if (current) open(current, to);
					return true;
				}
				case 'r':
					// The revision on screen, or else the previous one, like `helm rollback` without a revision.
					rollback(release, opened ? (tab === 'history' ? history[historyCursor]?.revision : (revision ?? undefined)) : undefined);
					return true;
				case '/':
					if (opened) return false;
					filterInput?.focus();
					return true;
				case 'f':
					if (!app.details) return false;
					app.maximized = !app.maximized;
					return true;
				case 'Escape':
					if (app.details) {
						app.details = null;
						app.maximized = false;
					} else if (opened) {
						back();
					} else if (filter) {
						filter = '';
					}
					return true;
			}
			return false;
		})();
		if (handled) e.preventDefault();
	}

	function blurEditor() {
		(document.activeElement as HTMLElement | null)?.blur();
	}
</script>

<svelte:window {onkeydown} />

{#snippet status(value: string)}
	{@const health = statusHealth(value)}
	<span class="status" class:muted={!health}>
		{#if health}<HealthIcon {health} />{/if}
		{value}
	</span>
{/snippet}

<section>
	{#if opened}
		<header>
			<button class="back" onclick={back} title="Back to releases (Esc)">← Releases</button>
			<h1 class="mono">{opened.name}</h1>
			<span class="scope">{opened.namespace}</span>
			{#if shown}{@render status(shown.status)}{/if}
			<span class="spacer"></span>
			{#if history.length > 1}
				<label class="revision">
					Revision
					<select
						value={revision ?? history[0]?.revision}
						onchange={(e) => viewRevision(Number(e.currentTarget.value))}
					>
						{#each history as h (h.revision)}
							<option value={h.revision}>{h.revision} · {h.status}</option>
						{/each}
					</select>
				</label>
			{/if}
			<button
				onclick={() => rollback(latest, revision ?? undefined)}
				disabled={!helm || !latest || (revision === null && history.length < 2)}
				title={helm ? (revision === null ? 'Roll back to the previous revision (r)' : `Roll back to revision ${revision}`) : 'Needs the helm command'}
			>
				Roll back{revision !== null ? ` to ${revision}` : ''}
			</button>
			<button class="danger" onclick={() => uninstall(latest)} disabled={!helm || !latest} title={helm ? 'Uninstall (Ctrl+D)' : 'Needs the helm command'}>
				Uninstall
			</button>
		</header>

		<nav>
			{#each TABS as [id, label, key] (id)}
				<button class:active={tab === id} onclick={() => (tab = id)}>{label} <kbd>{key}</kbd></button>
			{/each}
			<span class="spacer"></span>
			{#if tab === 'values'}
				<div class="toggle" role="group" aria-label="Values">
					<button class:active={!allValues} onclick={() => (allValues = false)}>Given</button>
					<button class:active={allValues} onclick={() => (allValues = true)}>With chart defaults</button>
				</div>
			{/if}
		</nav>

		{#if revision !== null}
			<div class="note bar">
				Showing revision {revision}, not the deployed one.
				<button class="link" onclick={() => (revision = null)}>Show the latest</button>
			</div>
		{/if}
		{#if helm === null}
			<div class="note bar">Install the <code>helm</code> command where Kubri runs to roll back or uninstall releases.</div>
		{/if}
		{#if detailsError}
			<div class="error">{detailsError}</div>
		{/if}

		<div class="pane" class:fill={tab === 'values' || tab === 'manifest'} bind:this={scroller}>
			{#if !shown}
				{#if !detailsError}<p class="muted pad">Loading…</p>{/if}
			{:else if tab === 'overview'}
				<dl class="facts">
					<dt>Status</dt>
					<dd>{@render status(shown.status)}</dd>
					<dt>Revision</dt>
					<dd>{shown.revision}</dd>
					<dt>Chart</dt>
					<dd class="mono">{chartLabel(shown)}</dd>
					<dt>App version</dt>
					<dd class="mono">{shown.appVersion || '—'}</dd>
					<dt>Updated</dt>
					<dd>
						{#if shown.updated}{age(shown.updated, now)} ago <span class="muted">· {new Date(shown.updated).toLocaleString()}</span>{:else}—{/if}
					</dd>
					{#if shown.description}
						<dt>Description</dt>
						<dd>{shown.description}</dd>
					{/if}
				</dl>

				<h3>Objects <span class="count">{objects.length}</span></h3>
				{#if objects.length}
					<ul class="objects">
						{#each objects as o, i (i)}
							{@const resource = manifestResource(app.resources, o)}
							<li>
								<span class="kind">{o.kind}</span>
								{#if resource}
									<button class="link mono" onclick={() => openObject(o)}>{o.name}</button>
								{:else}
									<span class="mono" title="The cluster does not serve this kind">{o.name}</span>
								{/if}
								{#if o.namespace && o.namespace !== opened.namespace}<span class="muted">in {o.namespace}</span>{/if}
							</li>
						{/each}
					</ul>
				{:else}
					<p class="muted">The manifest creates no objects.</p>
				{/if}

				<h3>Notes</h3>
				{#if shown.notes.trim()}
					<pre class="notes">{shown.notes.trim()}</pre>
				{:else}
					<p class="muted">The chart has no notes.</p>
				{/if}
			{:else if tab === 'values'}
				{#if valuesText}
					<YamlEditor value={valuesText} readonly oncancel={blurEditor} />
				{:else}
					<div class="pad">
						<p class="muted">
							{allValues ? 'The chart has no values.' : 'No values were given at install or upgrade, so the chart defaults apply.'}
						</p>
						{#if !allValues}<button class="link" onclick={() => (allValues = true)}>Show the chart defaults</button>{/if}
					</div>
				{/if}
			{:else if tab === 'manifest'}
				<YamlEditor value={shown.manifest} readonly oncancel={blurEditor} />
			{:else}
				<table>
					<thead>
						<tr>
							<th class="numeric">Rev</th>
							<th>Status</th>
							<th>Chart</th>
							<th>App version</th>
							<th>Updated</th>
							<th>Description</th>
							<th></th>
						</tr>
					</thead>
					<tbody>
						{#each history as h, i (h.revision)}
							<tr
								class:cursor={i === historyCursor}
								class:viewing={h.revision === shown.revision}
								onclick={() => (historyCursor = i)}
								ondblclick={() => viewRevision(h.revision)}
							>
								<td class="numeric">{h.revision}</td>
								<td>{@render status(h.status)}</td>
								<td class="mono">{chartLabel(h)}</td>
								<td class="mono">{h.appVersion}</td>
								<td class="muted">{age(h.updated, now)}</td>
								<td class="wrap">{h.description}</td>
								<td class="actions">
									<button class="small" onclick={() => viewRevision(h.revision)}>View</button>
									{#if canRollBack(h.revision)}
										<button class="small" onclick={() => rollback(latest, h.revision)}>Roll back</button>
									{/if}
								</td>
							</tr>
						{/each}
					</tbody>
				</table>
			{/if}
		</div>
	{:else}
		<header>
			<h1>Helm</h1>
			<span class="scope">{namespace ?? 'All namespaces'}</span>
			{#if releases}<span class="count">{visible.length}</span>{/if}
			<span class="spacer"></span>
			<input class="filter" bind:this={filterInput} bind:value={filter} placeholder="Filter releases…" />
			<button onclick={() => reloads++}>Reload</button>
		</header>

		{#if error}
			<div class="error">{error}</div>
		{/if}

		<div class="pane" bind:this={scroller}>
			{#if releases && visible.length}
				<table>
					<thead>
						<tr>
							<th>Name</th>
							{#if !namespace}<th>Namespace</th>{/if}
							<th class="numeric">Rev</th>
							<th>Status</th>
							<th>Chart</th>
							<th>App version</th>
							<th>Updated</th>
						</tr>
					</thead>
					<tbody>
						{#each visible as r, i (releaseKey(r))}
							<tr
								class:cursor={i === cursor}
								onclick={() => (cursorKey = releaseKey(r))}
								ondblclick={() => open(r)}
								oncontextmenu={(e) => openMenu(e, r)}
							>
								<td class="mono name"><button class="link" onclick={() => open(r)}>{r.name}</button></td>
								{#if !namespace}<td class="muted">{r.namespace}</td>{/if}
								<td class="numeric">{r.revision}</td>
								<td>{@render status(r.status)}</td>
								<td class="mono">{chartLabel(r)}</td>
								<td class="mono">{r.appVersion}</td>
								<td class="muted">{age(r.updated, now)}</td>
							</tr>
						{/each}
					</tbody>
				</table>
			{:else if releases}
				<div class="empty">
					{#if filter}
						<p>No release matches the filter.</p>
					{:else}
						<p>No Helm releases{namespace ? ` in ${namespace}` : ''}.</p>
						<p class="muted">Kubri reads the release records Helm 3 and 4 keep in Secrets or ConfigMaps.</p>
					{/if}
				</div>
			{:else if !error}
				<p class="muted pad">Loading…</p>
			{/if}
		</div>
	{/if}
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
		flex-wrap: wrap;
		align-items: center;
		gap: 8px 10px;
		padding: 14px 20px 10px;
	}
	h1 {
		margin: 0;
		font-size: 20px;
	}
	h1.mono {
		font-family: var(--mono);
		font-size: 18px;
	}
	.scope,
	.count {
		padding: 1px 8px;
		border-radius: 999px;
		background: var(--surface-2);
		font-size: 12px;
		font-weight: 600;
	}
	.spacer {
		flex: 1;
	}
	.filter {
		width: 200px;
		padding: 4px 8px;
		font-size: 13px;
	}
	header button,
	.small {
		padding: 4px 10px;
		border: 1px solid var(--border);
		border-radius: 6px;
		background: var(--surface-1);
		font-size: 12px;
	}
	header button:disabled {
		opacity: 0.5;
	}
	.small:hover {
		border-color: var(--accent);
	}
	header button.danger:not(:disabled):hover {
		border-color: var(--bad);
		color: var(--bad);
	}
	.back {
		color: var(--text-muted);
	}
	.revision {
		display: flex;
		align-items: center;
		gap: 6px;
		font-size: 12px;
		color: var(--text-muted);
	}
	.revision select {
		padding: 3px 6px;
		font-size: 12px;
	}
	nav {
		display: flex;
		align-items: center;
		gap: 4px;
		padding: 0 16px;
		border-bottom: 1px solid var(--border-subtle);
	}
	nav > button {
		padding: 7px 10px;
		border: none;
		border-bottom: 2px solid transparent;
		background: none;
		color: var(--text-muted);
		font-size: 13px;
	}
	nav > button.active {
		border-bottom-color: var(--accent);
		color: var(--text);
		font-weight: 600;
	}
	.toggle {
		display: flex;
		border: 1px solid var(--border);
		border-radius: 6px;
		overflow: hidden;
	}
	.toggle button {
		padding: 3px 10px;
		border: none;
		background: var(--surface-1);
		font-size: 12px;
		color: var(--text-muted);
	}
	.toggle button.active {
		background: color-mix(in srgb, var(--accent) 18%, var(--surface-1));
		color: var(--accent-strong);
		font-weight: 600;
	}
	.error {
		margin: 8px 20px;
		padding: 8px 12px;
		border-radius: 8px;
		background: color-mix(in srgb, var(--bad) 12%, var(--surface-1));
		color: var(--bad);
	}
	.note {
		padding: 6px 10px;
		border-radius: 6px;
		background: color-mix(in srgb, var(--warn) 12%, transparent);
		font-size: 13px;
	}
	.note.bar {
		margin: 8px 20px 0;
	}
	.pane {
		flex: 1;
		min-height: 0;
		overflow: auto;
		padding: 0 12px 16px;
	}
	.pane.fill {
		display: flex;
		flex-direction: column;
		overflow: hidden;
		padding: 0;
	}
	.pane.fill > :global(.editor) {
		flex: 1;
	}
	.pad {
		padding: 16px 20px;
	}
	.facts {
		display: grid;
		grid-template-columns: max-content minmax(0, 1fr);
		gap: 6px 18px;
		margin: 14px 8px;
		font-size: 13px;
	}
	.facts dt {
		color: var(--text-muted);
	}
	.facts dd {
		margin: 0;
	}
	h3 {
		display: flex;
		align-items: center;
		gap: 6px;
		margin: 20px 8px 8px;
		font-size: 11px;
		font-weight: 600;
		letter-spacing: 0.06em;
		text-transform: uppercase;
		color: var(--text-muted);
	}
	.objects {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
		gap: 2px 16px;
		margin: 0 8px;
		padding: 0;
		list-style: none;
		font-size: 13px;
	}
	.objects li {
		display: flex;
		align-items: baseline;
		gap: 6px;
		min-width: 0;
		padding: 2px 0;
	}
	.kind {
		flex: none;
		color: var(--text-muted);
		font-size: 11px;
		font-weight: 600;
		letter-spacing: 0.04em;
		text-transform: uppercase;
	}
	.notes {
		margin: 0 8px;
		padding: 10px 12px;
		border: 1px solid var(--border-subtle);
		border-radius: 8px;
		background: var(--surface-1);
		font-family: var(--mono);
		font-size: 12px;
		white-space: pre-wrap;
		overflow-wrap: anywhere;
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
	}
	td.wrap {
		min-width: 200px;
		white-space: normal;
	}
	tbody tr:hover {
		background: var(--surface-1);
	}
	tbody tr.cursor {
		background: color-mix(in srgb, var(--accent) 16%, transparent);
		box-shadow: inset 3px 0 0 var(--accent);
	}
	tr.viewing td:first-child {
		font-weight: 700;
	}
	.actions {
		display: flex;
		gap: 6px;
		justify-content: flex-end;
	}
	.small {
		padding: 1px 8px;
		font-size: 11.5px;
	}
	.status {
		display: inline-flex;
		align-items: center;
		gap: 6px;
	}
	.numeric {
		text-align: right;
		font-variant-numeric: tabular-nums;
	}
	.mono {
		font-family: var(--mono);
		font-size: 12.5px;
	}
	.muted {
		color: var(--text-muted);
	}
	.link {
		padding: 0;
		border: none;
		background: none;
		color: inherit;
		font: inherit;
		text-align: left;
	}
	.link:hover {
		color: var(--accent-strong);
		text-decoration: underline;
	}
	.note .link,
	.pad .link {
		color: var(--accent-strong);
		text-decoration: underline;
	}
	.empty {
		padding: 40px 24px;
		text-align: center;
	}
	.empty p {
		margin: 4px 0;
	}
	kbd {
		margin-left: 4px;
		padding: 0 4px;
		border: 1px solid var(--border);
		border-radius: 4px;
		background: var(--surface-2);
		font-family: var(--mono);
		font-size: 10.5px;
	}
	code {
		font-family: var(--mono);
		font-size: 12px;
	}
</style>
