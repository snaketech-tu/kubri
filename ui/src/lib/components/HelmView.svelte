<script lang="ts">
	import { parts, plural, t } from '../i18n';
	import { tick } from 'svelte';
	import { stringify } from 'yaml';
	import { backend, type ApplyItem, type HelmRelease, type HelmReleaseDetails } from '../api';
	import { age, errorMessage } from '../format';
	import {
		chartLabel,
		filterReleases,
		hasValues,
		manifestDocuments,
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
	import DiffView from './DiffView.svelte';
	import HealthIcon from './HealthIcon.svelte';
	import HelmChangeView from './HelmChangeView.svelte';
	import HelmCompare from './HelmCompare.svelte';
	import PageHeader from './PageHeader.svelte';
	import SearchField from './SearchField.svelte';
	import Select from './Select.svelte';
	import Phrase from './Phrase.svelte';
	import YamlEditor from './YamlEditor.svelte';

	type Tab = 'overview' | 'values' | 'manifest' | 'history' | 'compare' | 'drift';
	const TABS: [Tab, string, string][] = [
		['overview', t('Overview'), 'n'],
		['values', t('Values'), 'v'],
		['manifest', t('Manifest'), 'm'],
		['history', t('History'), 'h'],
		['compare', t('Compare'), 'c'],
		['drift', t('Drift'), 'x']
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
	/** Installing a chart, or upgrading the opened release. */
	let changing = $state<'install' | 'upgrade' | null>(null);
	/** The revision the shown one is compared with; null for the one before it. */
	let compareWith = $state<number | null>(null);
	let compared = $state<HelmReleaseDetails | null>(null);
	let compareError = $state<string | null>(null);
	/** Where the live objects differ from the deployed revision, found with a server-side dry run. */
	let drift = $state<{ key: string; items: ApplyItem[] | null; error: string | null } | null>(null);

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

	const compareTo = $derived(
		details
			? (compareWith ??
					previousRevision(history, details.revision) ??
					history.find((h) => h.revision !== details!.revision)?.revision ??
					null)
			: null
	);
	$effect(() => {
		const target = opened;
		const rev = compareTo;
		if (!target || tab !== 'compare' || rev === null) return;
		let cancelled = false;
		compareError = null;
		backend.helmRelease(target.namespace, target.name, rev).then(
			(d) => !cancelled && (compared = d),
			(err) => !cancelled && (compareError = errorMessage(err))
		);
		return () => (cancelled = true);
	});

	$effect(() => {
		void reloads;
		const d = shown;
		if (tab !== 'drift' || !d || revision !== null) return;
		const key = `${releaseKey(d)}@${d.revision}#${reloads}`;
		if (drift?.key === key) return;
		drift = { key, items: null, error: null };
		backend
			.apply({
				objects: manifestDocuments(d.manifest),
				namespace: d.namespace,
				dryRun: true,
				context: app.cluster?.context ?? null
			})
			.then(
				(items) => drift?.key === key && (drift = { key, items, error: null }),
				(err) => drift?.key === key && (drift = { key, items: null, error: errorMessage(err) })
			);
	});
	const drifted = $derived(drift?.items?.filter((i) => i.outcome !== 'unchanged') ?? []);

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
						cell('Age', age(row.updated, app.now()))
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
			compareWith = null;
			compared = null;
			drift = null;
		}
		tab = to;
	}

	/** A release's menu in the list, with the keys the list uses. */
	function openMenu(e: MouseEvent, release: HelmRelease) {
		e.preventDefault();
		cursorKey = releaseKey(release);
		const actions: MenuAction[] = TABS.map(([to, label, key]) => ({
			group: 'View',
			label: to === 'overview' ? t('Open') : label,
			keys: [key],
			hint: to === 'overview' ? 'enter' : undefined,
			run: () => open(release, to)
		}));
		if (helm) {
			actions.push(
				{ group: 'Change', label: t('Upgrade…'), keys: ['u'], run: () => upgrade(release) },
				{ group: 'Change', label: t('Roll back…'), keys: ['r'], run: () => void rollback(release) },
				{ group: 'Delete', label: t('Uninstall…'), keys: ['ctrl+d'], danger: true, run: () => uninstall(release) }
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

	function revisionOptions(releases: HelmRelease[]) {
		return releases.map((h) => ({ value: h.revision, label: `${h.revision} · ${h.status}` }));
	}

	function viewRevision(rev: number) {
		const newest = history[0]?.revision;
		revision = rev === newest ? null : rev;
		compareWith = null;
		compared = null;
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
			app.toast(t('{name} has no earlier revision to roll back to', { name: release.name }), 'bad');
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

	function upgrade(release: HelmRelease | undefined) {
		if (!release || !helm) return;
		open(release, tab);
		revision = null;
		changing = 'upgrade';
	}

	/** After an install or upgrade: the release, as it now is. */
	function changed(release: { namespace: string; name: string }) {
		changing = null;
		opened = release;
		cursorKey = releaseKey(release);
		revision = null;
		details = null;
		drift = null;
		tab = 'overview';
		reloads++;
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

	function compare(rev: number) {
		compareWith = rev;
		compared = null;
		tab = 'compare';
	}

	/** Whether the open release can go back to revision `target`. */
	function canRollBack(target: number) {
		return !!helm && !!latest && target !== latest.revision;
	}

	function onkeydown(e: KeyboardEvent) {
		if (app.overlayOpen || changing) return;
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
				case 'u':
					upgrade(release);
					return true;
				case 'i':
					if (opened || !helm) return false;
					changing = 'install';
					return true;
				case 'n':
				case 'v':
				case 'm':
				case 'h':
				case 'c':
				case 'x': {
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
	{#if changing === 'install'}
		<HelmChangeView oncancel={() => (changing = null)} ondone={changed} />
	{:else if changing === 'upgrade'}
		{#if shown && revision === null}
			<HelmChangeView release={shown} oncancel={() => (changing = null)} ondone={changed} />
		{:else}
			<p class="muted pad">{t('Loading…')}</p>
		{/if}
	{:else if opened}
		<PageHeader
			crumbs={[{ label: 'Helm', onclick: back, title: t('Back to releases (Esc)') }, opened.name]}
			title={opened.name}
			mono
			subtitle={`${t('Release in {namespace}', { namespace: opened.namespace })}${shown ? ` · ${chartLabel(shown)}` : ''}`}
		>
			{#if shown}<span class="tag">{@render status(shown.status)}</span>{/if}
			{#snippet tools()}
				{#if history.length > 1}
					<label class="revision">
						{t('Revision')}
						<Select value={revision ?? history[0]?.revision} options={revisionOptions(history)} onchange={viewRevision} compact />
					</label>
				{/if}
				<button class="btn" onclick={() => upgrade(latest)} disabled={!helm || !latest} title={helm ? t('Upgrade with changed values or another chart version (u)') : t('Needs the helm command')}>
					{t('Upgrade')}
				</button>
				<button
					class="btn"
					onclick={() => rollback(latest, revision ?? undefined)}
					disabled={!helm || !latest || (revision === null && history.length < 2)}
					title={helm ? (revision === null ? t('Roll back to the previous revision (r)') : t('Roll back to revision {revision}', { revision })) : t('Needs the helm command')}
				>
					{revision !== null ? t('Roll back to {revision}', { revision }) : t('Roll back')}
				</button>
				<button class="btn danger" onclick={() => uninstall(latest)} disabled={!helm || !latest} title={helm ? t('Uninstall (Ctrl+D)') : t('Needs the helm command')}>
					{t('Uninstall')}
				</button>
			{/snippet}
		</PageHeader>

		{#if revision !== null}
			<div class="notice warn" role="status">
				{t('Showing revision {revision}, not the deployed one.', { revision })}
				<button class="link" onclick={() => (revision = null)}>{t('Show the latest')}</button>
			</div>
		{/if}
		{#if helm === null}
			<div class="notice warn">
				<Phrase text={t('Install the {helm} command where Kubri runs to upgrade, roll back or uninstall releases.')} code={{ helm: 'helm' }} />
			</div>
		{/if}
		{#if detailsError}
			<div class="notice" role="alert">{detailsError}</div>
		{/if}

		<div class="card">
			<div class="tabs" role="tablist" aria-label={t('Release')}>
				{#each TABS as [id, label, key] (id)}
					<button role="tab" aria-selected={tab === id} onclick={() => (tab = id)}>{label} <kbd>{key}</kbd></button>
				{/each}
				<span class="spacer"></span>
				{#if tab === 'values'}
					<div class="segmented" role="group" aria-label={t('Values')}>
						<button aria-pressed={!allValues} onclick={() => (allValues = false)}>{t('Given')}</button>
						<button aria-pressed={allValues} onclick={() => (allValues = true)}>{t('With chart defaults')}</button>
					</div>
				{/if}
			</div>
			<div class="pane" class:fill={tab === 'values' || tab === 'manifest'} bind:this={scroller}>
				{#if !shown}
					{#if !detailsError}<p class="muted pad">{t('Loading…')}</p>{/if}
				{:else if tab === 'overview'}
					<dl class="facts">
						<dt>{t('Status')}</dt>
						<dd>{@render status(shown.status)}</dd>
						<dt>{t('Revision')}</dt>
						<dd>{shown.revision}</dd>
						<dt>{t('Chart')}</dt>
						<dd class="mono">{chartLabel(shown)}</dd>
						<dt>{t('App version')}</dt>
						<dd class="mono">{shown.appVersion || '—'}</dd>
						<dt>{t('Updated')}</dt>
						<dd>
							{#if shown.updated}{t('{age} ago', { age: age(shown.updated, app.snapshot?.taken ?? now) })} <span class="muted">· {new Date(shown.updated).toLocaleString()}</span>{:else}—{/if}
						</dd>
						{#if shown.description}
							<dt>{t('Description')}</dt>
							<dd>{shown.description}</dd>
						{/if}
					</dl>

					<h3>{t('Objects')} <span class="count">{objects.length}</span></h3>
					{#if objects.length}
						<ul class="objects">
							{#each objects as o, i (i)}
								{@const resource = manifestResource(app.resources, o)}
								<li>
									<span class="kind">{o.kind}</span>
									{#if resource}
										<button class="link mono" onclick={() => openObject(o)}>{o.name}</button>
									{:else}
										<span class="mono" title={t('The cluster does not serve this kind')}>{o.name}</span>
									{/if}
									{#if o.namespace && o.namespace !== opened.namespace}<span class="muted">{t('in {namespace}', { namespace: o.namespace })}</span>{/if}
								</li>
							{/each}
						</ul>
					{:else}
						<p class="muted">{t('The manifest creates no objects.')}</p>
					{/if}

					<h3>{t('Notes')}</h3>
					{#if shown.notes.trim()}
						<pre class="notes">{shown.notes.trim()}</pre>
					{:else}
						<p class="muted">{t('The chart has no notes.')}</p>
					{/if}
				{:else if tab === 'values'}
					{#if valuesText}
						<YamlEditor value={valuesText} readonly oncancel={blurEditor} />
					{:else}
						<div class="pad">
							<p class="muted">
								{allValues ? t('The chart has no values.') : t('No values were given at install or upgrade, so the chart defaults apply.')}
							</p>
							{#if !allValues}<button class="link" onclick={() => (allValues = true)}>{t('Show the chart defaults')}</button>{/if}
						</div>
					{/if}
				{:else if tab === 'manifest'}
					<YamlEditor value={shown.manifest} readonly oncancel={blurEditor} />
				{:else if tab === 'compare'}
					<div class="pad">
						{#if history.length < 2}
							<p class="muted">{t('The release has one revision, so there is nothing to compare it with.')}</p>
						{:else}
							<div class="compare-head">
								<label class="revision">
									{t('Changes from revision')}
									<Select
										value={compareTo ?? undefined}
										options={revisionOptions(history.filter((h) => h.revision !== shown.revision))}
										onchange={(r) => {
											compareWith = r;
											compared = null;
										}}
										compact
									/>
									{t('to revision {revision}', { revision: shown.revision })}
								</label>
								{#if compareTo !== null && canRollBack(compareTo)}
									<button class="btn small" onclick={() => rollback(latest, compareTo ?? undefined)}>{t('Roll back to {compareTo}', { compareTo })}</button>
								{/if}
							</div>
							{#if compareError}
								<p role="alert">{compareError}</p>
							{:else if compared && compared.revision === compareTo}
								<HelmCompare
									namespace={shown.namespace}
									before={{ values: compared.values, manifest: compared.manifest }}
									after={{ values: shown.values, manifest: shown.manifest }}
								/>
							{:else}
								<p class="muted">{t('Loading…')}</p>
							{/if}
						{/if}
					</div>
				{:else if tab === 'drift'}
					<div class="pad">
						{#if revision !== null}
							<p class="muted">{t('Drift compares the cluster with the deployed revision.')}</p>
							<button class="link" onclick={() => (revision = null)}>{t('Show the latest')}</button>
						{:else if !drift || (!drift.items && !drift.error)}
							<p class="muted">{t('Comparing the live objects with the release…')}</p>
						{:else if drift.error}
							<p role="alert">{drift.error}</p>
						{:else if !drifted.length}
							<p>
								<HealthIcon health="ok" />
								{plural(drift.items?.length ?? 0, 'The {n} object in the cluster matches the release.', 'The {n} objects in the cluster match the release.')}
							</p>
							<p class="muted">
								<Phrase text={t('Checked with a server-side dry run of the release manifest, like {command}.')} code={{ command: 'kubectl diff --server-side' }} />
							</p>
						{:else}
							<p>
								{t('{n} of {total} objects differ from the release. An upgrade or a rollback puts them back.', { n: drifted.length, total: drift.items?.length ?? 0 })}
							</p>
							{#each drifted as item (item.index)}
								<div class="drift">
									<div class="drift-head">
										<span class="kind">{item.kind}</span>
										<span class="mono">{item.name}</span>
										<span class="muted">
											{item.outcome === 'changed'
												? t('changed in the cluster')
												: item.outcome === 'created' || item.outcome === 'unchecked'
													? t('missing from the cluster')
													: t('could not be checked')}
										</span>
									</div>
									{#if item.outcome === 'failed'}
										<p class="muted">{item.message}</p>
									{:else if item.outcome === 'changed'}
										<DiffView
											before={item.before}
											after={item.after}
											changed={t('lines between the cluster and the release')}
											maxHeight="360px"
										/>
									{/if}
								</div>
							{/each}
						{/if}
						<p><button class="link" onclick={() => (drift = null)}>{t('Check again')}</button></p>
					</div>
				{:else}
					<table>
						<thead>
							<tr>
								<th class="numeric">{t('Rev')}</th>
								<th>{t('Status')}</th>
								<th>{t('Chart')}</th>
								<th>{t('App version')}</th>
								<th>{t('Updated')}</th>
								<th>{t('Description')}</th>
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
									<td class="muted">{age(h.updated, app.snapshot?.taken ?? now)}</td>
									<td class="wrap">{h.description}</td>
									<td>
										<div class="actions">
											<button class="btn small" onclick={() => viewRevision(h.revision)}>{t('View')}</button>
											{#if h.revision !== shown.revision}
												<button class="btn small" onclick={() => compare(h.revision)}>{t('Compare')}</button>
											{/if}
											{#if canRollBack(h.revision)}
												<button class="btn small" onclick={() => rollback(latest, h.revision)}>{t('Roll back')}</button>
											{/if}
										</div>
									</td>
								</tr>
							{/each}
						</tbody>
					</table>
				{/if}
			</div>
		</div>
	{:else}
		<PageHeader
			crumbs={[t('Helm')]}
			title={t('Helm releases')}
			count={releases ? visible.length : null}
			subtitle={namespace ? t('In {namespace}', { namespace }) : t('All namespaces')}
		>
			{#snippet tools()}
				<button class="btn" onclick={() => reloads++} title={t('Read the releases again')}>{t('Reload')}</button>
				<button class="btn" onclick={() => (changing = 'install')} disabled={!helm} title={helm ? t('Install a chart (i)') : t('Needs the helm command, see the note below')}>
					{t('Install chart')}
				</button>
			{/snippet}
		</PageHeader>

		{#if helm === undefined}
			<!-- The note about a missing helm and the list come together, so the list does not move. -->
			<p class="muted pad">{t('Loading…')}</p>
		{:else}
		{#if error}
			<div class="notice" role="alert">{error}</div>
		{/if}
		{#if helm === null}
			<div class="notice warn">
				{#each parts(t('Installing, upgrading and rolling back need the {helm} command, and Kubri did not find it on its PATH. Install Helm ({link}) where Kubri runs, then reload.')) as part, i (i)}{#if 'text' in part}{part.text}{:else if part.name === 'helm'}<code
							>helm</code
						>{:else}<a href="https://helm.sh/docs/intro/install/" target="_blank" rel="noreferrer">helm.sh/docs/intro/install</a>{/if}{/each}
			</div>
		{/if}

		<div class="card">
			<div class="toolbar">
				<SearchField bind:value={filter} bind:input={filterInput} label={t('Filter releases')} placeholder={t('Filter by name, namespace, chart or status')} />
			</div>
			<div class="pane" bind:this={scroller}>
				{#if releases && visible.length}
					<table>
						<thead>
							<tr>
								<th>{t('Name')}</th>
								{#if !namespace}<th>{t('Namespace')}</th>{/if}
								<th class="numeric">{t('Rev')}</th>
								<th>{t('Status')}</th>
								<th>{t('Chart')}</th>
								<th>{t('App version')}</th>
								<th>{t('Updated')}</th>
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
									<td class="muted">{age(r.updated, app.snapshot?.taken ?? now)}</td>
								</tr>
							{/each}
						</tbody>
					</table>
				{:else if releases}
					<div class="empty" role="status">
						{#if filter}
							<p>{t('No release matches the filter.')}</p>
						{:else}
							<p>{namespace ? t('No Helm releases in {namespace}.', { namespace }) : t('No Helm releases.')}</p>
							<p class="muted">{t('Kubri reads the release records Helm 3 and 4 keep in Secrets or ConfigMaps.')}</p>
							{#if helm && !app.readOnly && !app.cluster?.snapshot}
								<p class="install"><button class="btn" onclick={() => (changing = 'install')}>{t('Install chart…')}</button></p>
							{/if}
						{/if}
					</div>
				{:else if !error}
					<p class="muted pad">{t('Loading…')}</p>
				{/if}
			</div>
		</div>
		{/if}
	{/if}
</section>

<style>
	section {
		display: flex;
		flex-direction: column;
		min-height: 0;
		min-width: 0;
		padding: 0 24px 20px;
	}
	.tag {
		padding: 2px 8px;
		border-radius: 5px;
		background: var(--surface-2);
		font-size: 12px;
		font-weight: 600;
	}
	.spacer {
		flex: 1;
	}
	.revision {
		display: flex;
		align-items: center;
		gap: 6px;
		font-size: 12px;
		color: var(--text-muted);
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
	.toolbar {
		padding: 14px 16px 10px;
		border-bottom: 1px solid var(--border);
	}
	.tabs {
		display: flex;
		align-items: center;
		gap: 20px;
		padding: 0 16px;
		border-bottom: 1px solid var(--border);
	}
	.tabs > button {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		padding: 10px 0 9px;
		border: none;
		border-bottom: 2px solid transparent;
		background: none;
		color: var(--text-muted);
		font-size: 12.5px;
	}
	.tabs > button:hover {
		color: var(--text);
	}
	.tabs > button[aria-selected='true'] {
		border-bottom-color: var(--accent);
		color: var(--accent-strong);
		font-weight: 650;
	}
	.tabs .segmented {
		margin: 5px 0;
	}
	.pane {
		flex: 1;
		min-height: 0;
		overflow: auto;
		padding: 0 8px 12px;
	}
	.pane:has(> table) {
		padding: 0 0 12px;
	}
	th:first-child,
	td:first-child {
		padding-left: 16px;
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
	.compare-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
		margin-bottom: 4px;
	}
	.drift {
		display: grid;
		gap: 8px;
		margin: 10px 0;
	}
	.drift-head {
		display: flex;
		align-items: baseline;
		gap: 8px;
		font-size: 13px;
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
	td {
		height: 40px;
		padding: 4px 10px;
		border-bottom: 1px solid var(--border-subtle);
		white-space: nowrap;
	}
	td.wrap {
		min-width: 200px;
		white-space: normal;
	}
	tbody tr:hover {
		background: color-mix(in srgb, var(--surface-2) 55%, transparent);
	}
	tbody tr.cursor {
		background: color-mix(in srgb, var(--accent) 10%, var(--surface-1));
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
	.notice .link,
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
	.empty .install {
		margin-top: 14px;
	}
	.install .btn {
		padding: 7px 14px;
		border-color: var(--accent);
		background: var(--accent);
		color: var(--on-accent);
		font-size: 13px;
	}
	@media (max-width: 720px) {
		section {
			padding: 0 12px 12px;
		}
		.tabs {
			gap: 14px;
			overflow-x: auto;
		}
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
