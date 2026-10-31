<script lang="ts">
	import { untrack } from 'svelte';
	import { SvelteMap, SvelteSet } from 'svelte/reactivity';
	import { backend, type HistoryDetail, type HistoryEntry, type HistoryPage, type HistoryQuery } from '../api';
	import { CHANGE_GROUPS, RANGES, changeTag, changeTone, clock, timeline, when } from '../changes';
	import { errorMessage } from '../format';
	import { app } from '../state.svelte';
	import DiffView from './DiffView.svelte';
	import Icon from './Icon.svelte';
	import PageHeader from './PageHeader.svelte';
	import SearchField from './SearchField.svelte';
	import Select from './Select.svelte';

	const REFRESH_MS = 10_000;

	let page = $state.raw<HistoryPage | null>(null);
	/** Older pages loaded with "Show older". */
	let older = $state.raw<HistoryEntry[]>([]);
	let more = $state(false);
	let error = $state<string | null>(null);
	let text = $state('');
	let typed = $state('');
	let now = $state(Date.now());
	const groups = new SvelteSet<string>();
	const filter = $derived(app.changesFilter);
	let hours = $state(app.changesFilter.hours ?? 24);
	const object = $derived(filter.name ? { kind: filter.kind, name: filter.name } : null);
	const namespace = $derived(object ? filter.namespace : app.namespace || undefined);

	// Opening the page again with another filter starts from its range.
	$effect(() => {
		hours = filter.hours ?? (filter.name ? 0 : 24);
	});

	// Typing waits a moment before asking.
	$effect(() => {
		const value = text;
		const timer = setTimeout(() => (typed = value), 250);
		return () => clearTimeout(timer);
	});

	const query = $derived.by((): HistoryQuery => ({
		namespace,
		kind: object?.kind,
		name: object?.name,
		changes: CHANGE_GROUPS.filter((g) => groups.has(g.label)).flatMap((g) => g.kinds),
		text: typed.trim() || undefined,
		limit: 200
	}));
	const since = () => (hours ? Date.now() - hours * 3600_000 : undefined);

	$effect(() => {
		const asked = { ...query };
		void app.cluster?.context;
		void hours;
		let cancelled = false;
		let loading = false;
		page = null;
		older = [];
		error = null;
		const load = async () => {
			// Once older pages are loaded, refreshing would drop them.
			if (loading || older.length) return;
			loading = true;
			now = Date.now();
			try {
				const next = await backend.history({ ...asked, since: since() });
				if (!cancelled) {
					page = next;
					more = next.more;
					error = null;
				}
			} catch (err) {
				if (!cancelled) error = errorMessage(err);
			} finally {
				loading = false;
			}
		};
		untrack(load);
		const timer = setInterval(() => document.hidden || load(), REFRESH_MS);
		return () => {
			cancelled = true;
			clearInterval(timer);
		};
	});

	async function showOlder() {
		const all = [...(page?.entries ?? []), ...older];
		const last = all[all.length - 1];
		if (!last) return;
		try {
			const next = await backend.history({ ...query, since: since(), before: last.id });
			older = [...older, ...next.entries];
			more = next.more;
		} catch (err) {
			app.toast(errorMessage(err), 'bad');
		}
	}

	const entries = $derived([...(page?.entries ?? []), ...older]);
	const items = $derived(
		page ? timeline(entries, page.watched, hours ? now - hours * 3600_000 : null, now, page.recorder.recording, !more) : []
	);

	/** Diffs shown, by entry id; `null` while loading. */
	const details = new SvelteMap<number, HistoryDetail | null>();
	async function toggle(entry: HistoryEntry) {
		if (details.has(entry.id)) {
			details.delete(entry.id);
			return;
		}
		details.set(entry.id, null);
		try {
			details.set(entry.id, await backend.historyDetail(entry.id));
		} catch (err) {
			details.delete(entry.id);
			app.toast(errorMessage(err), 'bad');
		}
	}

	function open(entry: HistoryEntry) {
		app.openObject({ context: entry.context, resource: entry.resource, namespace: entry.namespace, name: entry.name }, entry.kind);
	}

	function showAll() {
		app.showChanges({ namespace: filter.namespace });
	}

	const title = $derived(object ? `${object.name}` : 'What changed');
	const where = $derived(object ? `this ${object.kind ?? 'object'}` : namespace ? `namespace ${namespace}` : 'all namespaces');
	const crumbs = $derived(
		object ? [{ label: 'What changed', onclick: showAll, title: 'All changes in the namespace' }, object.kind ?? 'Object'] : ['What changed']
	);
</script>

<section class="page">
	<PageHeader {crumbs} {title} mono={!!object} count={page ? entries.length : null} subtitle="Changes Kubri recorded in {app.cluster?.context ?? 'this context'}, {where}, while it was running">
		{#snippet tools()}
			{#if object}
				<button class="btn" onclick={showAll}>All changes</button>
			{/if}
		{/snippet}
	</PageHeader>

	{#if page && !page.recorder.recording}
		<div class="banner" role="status">
			<Icon name="pending" size={14} />
			<span>Not recording now: {page.recorder.reason ?? 'unknown reason'}.</span>
			{#if page.recorder.reason?.includes('turned off')}
				<button class="link" onclick={() => app.showConfig('changes')}>Settings</button>
			{/if}
		</div>
	{/if}
	{#if page?.recorder.skipped.length}
		<details class="skipped">
			<summary><Icon name="warning" size={14} /> {page.recorder.skipped.length} {page.recorder.skipped.length === 1 ? 'kind is' : 'kinds are'} not recorded</summary>
			<ul>
				{#each page.recorder.skipped as s (s)}<li>{s}</li>{/each}
			</ul>
		</details>
	{/if}
	{#if error}
		<div class="notice" role="alert">Could not read the changes: {error}</div>
	{/if}

	<div class="card">
		<div class="toolbar">
			<label>
				<span>Range</span>
				<Select
					bind:value={hours}
					options={[
						...RANGES.map((r) => ({ value: r.hours, label: r.label })),
						...(RANGES.some((r) => r.hours === hours)
							? []
							: [{ value: hours, label: `Last ${hours < 1 ? `${Math.round(hours * 60)} minutes` : `${hours} hours`}` }])
					]}
					aria-label="Range"
					compact
				/>
			</label>
			<SearchField bind:value={text} label="Find in the changes" placeholder="Name, kind or what changed…" key="" />
			<div class="groups" role="group" aria-label="Kinds of change">
				{#each CHANGE_GROUPS as g (g.label)}
					<button
						class="chip"
						class:on={groups.has(g.label)}
						aria-pressed={groups.has(g.label)}
						onclick={() => (groups.has(g.label) ? groups.delete(g.label) : groups.add(g.label))}>{g.label}</button
					>
				{/each}
			</div>
		</div>

		<div class="list">
			{#if !page && !error}
				<p class="muted pad">Reading the changes…</p>
			{:else if page && !entries.length && !items.length}
				<p class="muted pad">
					No changes recorded {hours ? `in the ${RANGES.find((r) => r.hours === hours)?.label.toLowerCase() ?? `last ${hours} hours`}` : 'yet'}.
					{#if page.recorder.recording && page.recorder.since}Kubri has been recording since {when(page.recorder.since, now)}.{/if}
				</p>
			{:else}
				<ol>
					{#each items as item (item.key)}
						{#if item.type === 'day'}
							<li class="day">{item.label}</li>
						{:else if item.type === 'gap'}
							<li class="gap">
								<Icon name="pending" size={13} />
								{#if item.from === null}
									Kubri was not watching before {when(item.to ?? now, now)}
								{:else if item.to === null}
									Kubri has not been watching since {when(item.from, now)}
								{:else}
									Kubri was not watching from {when(item.from, now)} to {when(item.to, now)}
								{/if}
							</li>
						{:else}
							{@const e = item.entry}
							{@const detail = details.get(e.id)}
							<li class="entry">
								<div class="row">
									<time datetime={new Date(e.at).toISOString()} title={new Date(e.at).toLocaleString()}>{clock(e.at)}</time>
									<span class="tag {changeTone(e.change)}">{changeTag(e.change)}</span>
									<span class="what">
										{#if e.change === 'deleted'}
											<span class="name">{e.name}</span>
										{:else}
											<button class="name" onclick={() => open(e)} title="Open {e.kind} {e.name}">{e.name}</button>
										{/if}
										<span class="muted">{e.kind}{e.namespace && !namespace ? ` · ${e.namespace}` : ''}</span>
									</span>
									<span class="summary">{e.summary}</span>
									{#if e.hasDiff}
										<button class="btn small" aria-expanded={details.has(e.id)} onclick={() => toggle(e)}>{details.has(e.id) ? 'Hide diff' : 'Diff'}</button>
									{/if}
								</div>
								{#if e.unwatched}
									<p class="note">Happened while Kubri was not watching; found when it started again at {clock(e.at)}.</p>
								{/if}
								{#if details.has(e.id)}
									<div class="diff">
										{#if detail}
											<DiffView before={detail.before} after={detail.after} changed="lines changed" unchanged="Only fields Kubri does not keep changed." />
											{#if e.kind === 'Secret' || e.kind === 'ConfigMap'}
												<p class="note">Values are never kept: each shows its size and a fingerprint that changes with it.</p>
											{/if}
										{:else}
											<p class="muted">Reading…</p>
										{/if}
									</div>
								{/if}
							</li>
						{/if}
					{/each}
				</ol>
				{#if more}
					<button class="btn more" onclick={showOlder}>Show older</button>
				{/if}
			{/if}
		</div>
	</div>
</section>

<style>
	.page {
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
	.toolbar {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 10px;
		padding: 10px 12px;
		border-bottom: 1px solid var(--border);
		font-size: 13px;
	}
	.toolbar label {
		display: flex;
		align-items: center;
		gap: 6px;
		color: var(--text-muted);
	}
	.toolbar :global(.search) {
		width: 240px;
		height: 32px;
	}
	.groups {
		display: flex;
		flex-wrap: wrap;
		gap: 6px;
	}
	.chip {
		padding: 4px 10px;
		border: 1px solid var(--border);
		border-radius: 999px;
		background: none;
		color: var(--text-muted);
		font: inherit;
		font-size: 12.5px;
	}
	.chip:hover {
		color: var(--text);
	}
	.chip.on {
		border-color: var(--accent);
		background: color-mix(in srgb, var(--accent) 14%, transparent);
		color: var(--text);
	}
	.list {
		flex: 1;
		min-height: 0;
		overflow: auto;
	}
	ol {
		margin: 0;
		padding: 0;
		list-style: none;
	}
	.day {
		position: sticky;
		top: 0;
		z-index: 1;
		padding: 8px 14px 6px;
		background: var(--surface-1);
		border-bottom: 1px solid var(--border);
		color: var(--text-muted);
		font-size: 12px;
		font-weight: 600;
	}
	.gap {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 7px 14px;
		border-bottom: 1px solid var(--border);
		background: repeating-linear-gradient(135deg, transparent 0 6px, color-mix(in srgb, var(--text-muted) 7%, transparent) 6px 12px);
		color: var(--text-muted);
		font-size: 12.5px;
		font-style: italic;
	}
	.entry {
		padding: 7px 14px;
		border-bottom: 1px solid var(--border);
		font-size: 13px;
	}
	.row {
		display: grid;
		grid-template-columns: max-content 72px minmax(160px, 260px) 1fr auto;
		align-items: baseline;
		gap: 10px;
	}
	time {
		min-width: 64px;
		color: var(--text-muted);
		white-space: nowrap;
		font-family: var(--mono);
		font-size: 12px;
	}
	.tag {
		justify-self: start;
		padding: 1px 7px;
		border-radius: 4px;
		background: var(--surface-2);
		color: var(--text-muted);
		font-size: 11.5px;
		font-weight: 600;
	}
	.tag.accent {
		background: color-mix(in srgb, var(--accent) 14%, transparent);
		color: var(--text);
	}
	.tag.warn {
		background: color-mix(in srgb, var(--warn) 16%, transparent);
		color: var(--text);
	}
	.tag.bad {
		background: color-mix(in srgb, var(--bad) 14%, transparent);
		color: var(--text);
	}
	.what {
		display: flex;
		flex-direction: column;
		min-width: 0;
	}
	.what .muted {
		font-size: 12px;
	}
	.name {
		padding: 0;
		border: none;
		background: none;
		color: var(--text);
		font: inherit;
		font-family: var(--mono);
		font-size: 12.5px;
		font-weight: 600;
		text-align: left;
		overflow-wrap: anywhere;
	}
	button.name {
		color: var(--accent);
	}
	button.name:hover {
		text-decoration: underline;
	}
	.summary {
		overflow-wrap: anywhere;
	}
	.small {
		padding: 2px 10px;
		font-size: 12px;
	}
	.note {
		margin: 4px 0 0;
		padding-left: 160px;
		color: var(--text-muted);
		font-size: 12px;
	}
	.diff {
		margin: 8px 0 4px;
		padding-left: 160px;
	}
	.more {
		margin: 12px 14px;
	}
	.banner,
	.notice {
		display: flex;
		align-items: center;
		gap: 8px;
		margin-bottom: 10px;
		padding: 9px 12px;
		border-radius: 8px;
		font-size: 13px;
	}
	.banner {
		background: var(--surface-2);
		color: var(--text-muted);
	}
	.notice {
		background: color-mix(in srgb, var(--bad) 10%, transparent);
		color: var(--bad);
	}
	.link {
		padding: 0;
		border: none;
		background: none;
		color: var(--accent);
		font: inherit;
	}
	.skipped {
		margin-bottom: 10px;
		color: var(--warn);
		font-size: 12.5px;
	}
	.skipped summary {
		display: flex;
		align-items: center;
		gap: 6px;
		cursor: pointer;
	}
	.skipped ul {
		margin: 6px 0 0;
		color: var(--text-muted);
	}
	.muted {
		color: var(--text-muted);
	}
	.pad {
		margin: 0;
		padding: 18px;
	}
	@media (max-width: 900px) {
		.row {
			grid-template-columns: max-content 64px 1fr;
		}
		.summary {
			grid-column: 2 / -1;
		}
		.note,
		.diff {
			padding-left: 0;
		}
	}
</style>
