<script module lang="ts">
	import type { SanitizerReport } from '../api';

	// The last scan per context and namespace, so coming back does not scan again at once.
	const scans = new Map<string, { report: SanitizerReport; at: number }>();
</script>

<script lang="ts">
	import { tick, untrack } from 'svelte';
	import { SvelteSet } from 'svelte/reactivity';
	import { backend } from '../api';
	import { age, errorMessage } from '../format';
	import { forControl } from '../keys';
	import {
		gradeTone,
		initialExpanded,
		sanitizerRows,
		scoreTone,
		totals,
		type LevelFilter,
		type SanitizerRow
	} from '../sanitizer';
	import { app, findResource } from '../state.svelte';
	import LevelIcon from './LevelIcon.svelte';

	const LEVELS: [LevelFilter, string][] = [
		['info', 'All'],
		['warning', 'Warnings and errors'],
		['error', 'Errors']
	];

	let report = $state.raw<SanitizerReport | null>(null);
	let scannedAt = $state(0);
	let scanning = $state(false);
	let error = $state<string | null>(null);
	let min = $state<LevelFilter>('warning');
	let filter = $state('');
	let cursorId = $state<string | null>(null);
	let now = $state(Date.now());
	const expanded = new SvelteSet<string>();
	let scroller = $state<HTMLDivElement>();
	let filterInput = $state<HTMLInputElement>();

	const namespace = $derived(app.namespace || null);
	const scanKey = $derived(`${app.cluster?.context}/${namespace ?? ''}`);

	function show(next: SanitizerReport, at: number) {
		report = next;
		scannedAt = at;
		expanded.clear();
		for (const id of initialExpanded(next)) expanded.add(id);
	}

	async function scan() {
		if (scanning) return;
		const key = scanKey;
		scanning = true;
		error = null;
		try {
			const next = await backend.sanitize(namespace);
			const at = Date.now();
			scans.set(key, { report: next, at });
			if (key === scanKey) show(next, at);
		} catch (err) {
			error = errorMessage(err);
		} finally {
			scanning = false;
		}
	}

	$effect(() => {
		const cached = scans.get(scanKey);
		untrack(() => (cached ? show(cached.report, cached.at) : scan()));
	});

	$effect(() => {
		const timer = setInterval(() => (now = Date.now()), 15_000);
		return () => clearInterval(timer);
	});

	const rows = $derived(report ? sanitizerRows(report, expanded, min, filter) : []);
	const cursor = $derived.by(() => {
		const i = rows.findIndex((r) => r.id === cursorId);
		return i === -1 ? 0 : i;
	});
	const current = $derived<SanitizerRow | undefined>(rows[cursor]);
	const sum = $derived(report ? totals(report) : null);

	$effect(() => {
		void cursorId;
		tick().then(() => scroller?.querySelector('.row.cursor')?.scrollIntoView({ block: 'nearest' }));
	});

	function move(delta: number) {
		if (!rows.length) return;
		cursorId = rows[Math.max(0, Math.min(rows.length - 1, cursor + delta))].id;
	}

	function toggle(id: string) {
		if (expanded.has(id)) expanded.delete(id);
		else expanded.add(id);
	}

	function openDetails(row: SanitizerRow | undefined) {
		if (row?.type !== 'finding') return;
		const resource = findResource(app.resources, row.section.resource);
		if (!resource) return;
		cursorId = row.id;
		const ns = resource.namespaced ? row.finding.namespace : null;
		app.showDetails(resource, { resource, namespace: ns, name: row.finding.name });
	}

	function sectionOf(row: SanitizerRow) {
		return rows.find((r) => r.type === 'section' && r.section === row.section)?.id ?? null;
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
		if (forControl(e) || e.ctrlKey || e.metaKey || e.altKey) return;
		const row = current;
		const handled = (() => {
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
					move(-rows.length);
					return true;
				case 'G':
					move(rows.length);
					return true;
				case 'ArrowRight':
				case 'l':
					if (row?.type === 'section' && !expanded.has(row.id)) expanded.add(row.id);
					else move(1);
					return true;
				case 'ArrowLeft':
				case 'h':
					if (!row) return true;
					if (row.type === 'section') expanded.delete(row.id);
					else cursorId = sectionOf(row);
					return true;
				case ' ':
					if (row) toggle(row.type === 'section' ? row.id : (sectionOf(row) ?? row.id));
					if (row?.type === 'finding') cursorId = sectionOf(row);
					return true;
				case 'Enter':
				case 'd':
					if (row?.type === 'section') toggle(row.id);
					else openDetails(row);
					return true;
				case 'R':
					scan();
					return true;
				case '/':
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
					} else if (filter) {
						filter = '';
					}
					return true;
			}
			return false;
		})();
		if (handled) e.preventDefault();
	}

	function label(n: number, one: string, many = `${one}s`) {
		return `${n} ${n === 1 ? one : many}`;
	}
</script>

<svelte:window {onkeydown} />

<section>
	<header>
		<h1>Sanitizer</h1>
		<span class="scope">{namespace ?? 'All namespaces'}</span>
		<span class="spacer"></span>
		<div class="toggle" role="group" aria-label="Levels shown">
			{#each LEVELS as [level, text] (level)}
				<button class:active={min === level} onclick={() => (min = level)}>{text}</button>
			{/each}
		</div>
		<input class="filter" bind:this={filterInput} bind:value={filter} placeholder="Filter findings…" />
		<button onclick={scan} disabled={scanning} title="Scan again (Shift+R)">{scanning ? 'Scanning…' : 'Scan again'}</button>
	</header>

	{#if error}
		<div class="error">{error}</div>
	{/if}

	{#if report && sum}
		<div class="summary">
			<div class="grade {gradeTone(report.grade)}" aria-label="Grade {report.grade}">{report.grade}</div>
			<div class="figures">
				<div class="score"><strong>{report.score}</strong> <span class="muted">/ 100</span></div>
				<div class="muted">
					{label(report.sections.length, 'kind')} · {label(sum.objects, 'object')} · scanned {age(new Date(scannedAt).toISOString(), now) || '0s'} ago
				</div>
			</div>
			<div class="counts">
				<span><LevelIcon level="error" /> {label(sum.error, 'object')} with errors</span>
				<span><LevelIcon level="warning" /> {label(sum.warning, 'object')} with warnings</span>
			</div>
			<p class="how muted">
				Each object scores 1 without warnings or errors, ½ with warnings and 0 with an error. Infos cost nothing.
				{#if !namespace}Nodes, volumes and cluster roles are included.{/if}
			</p>
		</div>
	{/if}

	<div class="list" bind:this={scroller} role="tree" aria-label="Sanitizer findings">
		{#each rows as row, i (row.id)}
			{#if row.type === 'section'}
				{@const s = row.section}
				{@const open = expanded.has(row.id)}
				<div
					class="row kind"
					class:cursor={i === cursor}
					role="treeitem"
					aria-selected={i === cursor}
					aria-expanded={row.shown ? open : undefined}
					tabindex="-1"
					onclick={() => {
						cursorId = row.id;
						toggle(row.id);
					}}
					onkeydown={() => {}}
				>
					<span class="chevron" class:open class:leaf={!row.shown}>›</span>
					<span class="kind-name">{s.kind}</span>
					{#if s.resource.group}<span class="group">{s.resource.group}</span>{/if}
					{#if s.score !== null}
						<span class="meter {scoreTone(s.score)}" role="meter" aria-valuemin="0" aria-valuemax="100" aria-valuenow={s.score} aria-label="{s.kind} score">
							<span class="fill" style:width="{s.score}%"></span>
						</span>
						<span class="value">{s.score}</span>
						<span class="tally">
							<span class:zero={!s.error}><LevelIcon level="error" /> {s.error}</span>
							<span class:zero={!s.warning}><LevelIcon level="warning" /> {s.warning}</span>
							<span class="muted">of {s.total}</span>
						</span>
					{:else}
						<span class="failed" title={s.failed}>Could not list: {s.failed}</span>
					{/if}
				</div>
			{:else}
				{@const f = row.finding}
				<div
					class="row finding"
					class:cursor={i === cursor}
					role="treeitem"
					aria-selected={i === cursor}
					tabindex="-1"
					onclick={() => (cursorId = row.id)}
					ondblclick={() => openDetails(row)}
					onkeydown={() => {}}
				>
					<div class="object">
						<LevelIcon level={f.level} />
						<button class="name link" tabindex="-1" onclick={() => openDetails(row)} title="Describe {row.section.kind} {f.name}">
							{f.namespace && !namespace ? `${f.namespace}/` : ''}{f.name}
						</button>
					</div>
					<ul>
						{#each f.issues as issue, j (j)}
							<li>
								<LevelIcon level={issue.level} size={8} />
								<span class="message">{issue.message}</span>
								{#if issue.container}<span class="container">{issue.container}</span>{/if}
								<span class="code">{issue.code}</span>
							</li>
						{/each}
					</ul>
				</div>
			{/if}
		{:else}
			<div class="empty">
				{#if scanning}
					Scanning {namespace ?? 'the cluster'}…
				{:else if report && filter}
					Nothing matches the filter.
				{:else if report}
					Nothing to check {namespace ? `in ${namespace}` : ''}.
				{/if}
			</div>
		{/each}
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
		flex-wrap: wrap;
		align-items: center;
		gap: 8px 10px;
		padding: 14px 20px 10px;
	}
	h1 {
		margin: 0;
		font-size: 20px;
	}
	.scope {
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
		width: 180px;
		padding: 4px 8px;
		font-size: 13px;
	}
	header > button {
		padding: 4px 10px;
		border: 1px solid var(--border);
		border-radius: 6px;
		background: var(--surface-1);
		font-size: 12px;
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
		margin: 0 20px 10px;
		padding: 8px 12px;
		border-radius: 8px;
		background: color-mix(in srgb, var(--bad) 12%, var(--surface-1));
		color: var(--bad);
	}
	.summary {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 10px 22px;
		margin: 0 20px 12px;
		padding: 14px 18px;
		border: 1px solid var(--border);
		border-radius: 12px;
		background: var(--surface-1);
	}
	.grade {
		display: grid;
		place-items: center;
		width: 56px;
		height: 56px;
		border-radius: 14px;
		font-size: 30px;
		font-weight: 700;
	}
	.grade.ok {
		background: color-mix(in srgb, var(--ok) 16%, transparent);
		box-shadow: inset 0 0 0 2px var(--ok);
	}
	.grade.warn {
		background: color-mix(in srgb, var(--warn-mark) 16%, transparent);
		box-shadow: inset 0 0 0 2px var(--warn-mark);
	}
	.grade.bad {
		background: color-mix(in srgb, var(--bad) 16%, transparent);
		box-shadow: inset 0 0 0 2px var(--bad);
	}
	.figures {
		display: flex;
		flex-direction: column;
		gap: 2px;
		font-size: 12.5px;
	}
	.score strong {
		font-size: 22px;
	}
	.counts {
		display: flex;
		flex-direction: column;
		gap: 4px;
		font-size: 13px;
	}
	.counts span,
	.tally span {
		display: inline-flex;
		align-items: center;
		gap: 5px;
	}
	.how {
		flex-basis: 100%;
		margin: 0;
		font-size: 12px;
	}
	.muted {
		color: var(--text-muted);
	}
	.list {
		flex: 1;
		overflow: auto;
		padding: 0 12px 16px;
		font-size: 13px;
	}
	.row {
		border-radius: 6px;
		outline: none;
	}
	.row:hover {
		background: var(--surface-1);
	}
	.row.cursor {
		background: color-mix(in srgb, var(--accent) 16%, transparent);
		box-shadow: inset 3px 0 0 var(--accent);
	}
	.kind {
		display: flex;
		align-items: center;
		gap: 8px;
		min-height: 32px;
		margin-top: 2px;
		padding: 0 12px 0 8px;
		cursor: default;
	}
	.chevron {
		width: 16px;
		color: var(--text-muted);
		font-size: 15px;
		line-height: 1;
		text-align: center;
		transition: transform 0.12s;
	}
	.chevron.open {
		transform: rotate(90deg);
	}
	.chevron.leaf {
		visibility: hidden;
	}
	.kind-name {
		min-width: 150px;
		font-weight: 600;
	}
	.group {
		margin-left: -2px;
		color: var(--text-muted);
		font-size: 11.5px;
	}
	/* The track is a light step of the fill's own color, so the state reads across the bar. */
	.meter {
		--tone: var(--ok);
		position: relative;
		flex: none;
		width: 120px;
		height: 8px;
		margin-left: auto;
		border-radius: 4px;
		background: color-mix(in srgb, var(--tone) 18%, var(--surface-2));
	}
	.meter.warn {
		--tone: var(--warn-mark);
	}
	.meter.bad {
		--tone: var(--bad);
	}
	.fill {
		position: absolute;
		inset: 0 auto 0 0;
		border-radius: 4px;
		background: var(--tone);
	}
	.value {
		width: 28px;
		font-variant-numeric: tabular-nums;
		font-weight: 600;
		text-align: right;
	}
	.tally {
		display: inline-flex;
		gap: 12px;
		min-width: 150px;
		font-size: 12px;
		font-variant-numeric: tabular-nums;
	}
	.tally .zero {
		opacity: 0.45;
	}
	.failed {
		margin-left: auto;
		overflow: hidden;
		color: var(--bad);
		font-size: 12px;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.finding {
		padding: 5px 12px 5px 34px;
	}
	.object {
		display: flex;
		align-items: center;
		gap: 7px;
	}
	.name {
		font-family: var(--mono);
		font-size: 12.5px;
	}
	.link {
		padding: 0;
		border: none;
		background: none;
		color: inherit;
		text-align: left;
	}
	.link:hover {
		color: var(--accent-strong);
		text-decoration: underline;
	}
	ul {
		margin: 3px 0 2px 17px;
		padding: 0;
		list-style: none;
	}
	li {
		display: flex;
		align-items: center;
		gap: 7px;
		padding: 1px 0;
		font-size: 12.5px;
	}
	.message {
		min-width: 0;
	}
	.container {
		padding: 0 6px;
		border-radius: 4px;
		background: var(--surface-2);
		font-family: var(--mono);
		font-size: 11px;
	}
	.code {
		color: var(--text-muted);
		font-family: var(--mono);
		font-size: 11px;
	}
	.empty {
		padding: 40px 24px;
		text-align: center;
		color: var(--text-muted);
	}
</style>
