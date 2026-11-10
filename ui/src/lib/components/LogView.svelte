<script lang="ts">
	import { tick, untrack } from 'svelte';
	import { backend, type KubeObject, type LogEvent, type LogLevel, type LogOptions, type LogSource, type ObjectRef } from '../api';
	import { copyLines, saveFile } from '../actions';
	import { containerCause } from '../causes';
	import { errorMessage } from '../format';
	import { dropdown } from '../dismiss';
	import { selectedText } from '../explain';
	import { forControl } from '../keys';
	import {
		appendInOrder,
		atLevel,
		cell,
		defaultColumns,
		fieldNames,
		fieldValue,
		foldRows,
		lineMatcher,
		parseQuery,
		pretty,
		rate,
		rowText,
		shortTime,
		type Row,
		type ViewLine
	} from '../logs';
	import { logRanges, sourceLabels, stripAnsi } from '../pods';
	import { plural, t } from '../i18n';
	import { app } from '../state.svelte';
	import { plainOptions, type SelectOption } from '../select';
	import CauseCard from './CauseCard.svelte';
	import Icon from './Icon.svelte';
	import LogRate from './LogRate.svelte';
	import Select from './Select.svelte';

	let {
		target,
		containers,
		previous: startPrevious = false,
		container: wanted = null,
		active = false,
		pod = null
	}: {
		target: ObjectRef;
		containers: string[];
		previous?: boolean;
		/** A container to show, asked for from Describe; a new object each time. */
		container?: { name: string; previous?: boolean } | null;
		/** The logs are on screen and take the k9s log keys. */
		active?: boolean;
		/** The pod itself, when the logs are of one pod: says why a container has none. */
		pod?: KubeObject | null;
	} = $props();

	/** Older lines are dropped beyond this, to keep the view fast. */
	const MAX_LINES = 10_000;
	const COLORS = ['#3b82f6', '#10b981', '#f59e0b', '#ec4899', '#8b5cf6', '#06b6d4', '#ef4444', '#84cc16'];
	// k9s `logger` settings: lines to start with, how far back, wrap, timestamps, autoscroll.
	const logger = untrack(() => app.k9s?.logger);
	const { ranges: RANGES, start: startRange } = logRanges(logger?.tail ?? 100, logger?.sinceSeconds ?? -1);
	/** The ranges k9s puts on 0 to 6: tail, head (here everything), then 1m to 1h. */
	const RANGE_KEYS = ['tail', 'all', '1m', '5m', '15m', '30m', '1h'];
	const LEVELS: SelectOption<LogLevel | ''>[] = [
		{ value: '', label: t('All levels') },
		{ value: 'info', label: t('Info and above') },
		{ value: 'warn', label: t('Warnings and errors') },
		{ value: 'error', label: t('Errors only') }
	];
	const LOKI_RANGES: [number, string][] = [
		[900, t('Last 15 minutes')],
		[3600, t('Last hour')],
		[6 * 3600, t('Last 6 hours')],
		[86400, t('Last day')],
		[7 * 86400, t('Last 7 days')]
	];

	let container = $state(untrack(() => wanted?.name ?? ''));
	let follow = $state(true);
	// Only the starting value; the checkbox owns it afterwards.
	let previous = $state(untrack(() => startPrevious));
	let timestamps = $state(logger?.showTime ?? false);
	let wrap = $state(logger?.textWrap ?? false);
	let range = $state(startRange);
	/** Lines from Loki instead of the cluster: history beyond the pods that exist now. */
	let loki = $state(false);
	let lokiSeconds = $state(3600);
	/** The LogQL being edited, and the one that ran; empty runs the object's own query. */
	let logql = $state('');
	let ranLogql = $state('');
	let lokiSource = $state('');
	let filter = $state('');
	/** Hide lines that do not match the search, or only highlight them and step through them. */
	let onlyMatches = $state(true);
	let minLevel = $state<LogLevel | ''>('');
	/** Count repeated lines and fold stack traces under their first line. */
	let fold = $state(true);
	let table = $state(false);
	/** Columns picked for the table; until then, the message and the most frequent fields. */
	let picked = $state<string[] | null>(null);
	/** Rows unfolded or pretty-printed, by the id of their first line. */
	let open = $state(new Set<number>());
	/** Pods left out of a workload's log. */
	let hiddenPods = $state(new Set<string>());
	/** The match in focus while stepping through matches with n and N. */
	let current = $state(-1);
	/** Keeps the newest line in view; k9s `s`. */
	let autoscroll = !logger?.disableAutoscroll;
	/** Ids of mark lines count down, apart from the log's own. */
	let marks = 0;

	$effect(() => {
		if (!wanted) return;
		container = wanted.name;
		if (wanted.previous !== undefined) previous = wanted.previous;
	});

	let lines = $state.raw<ViewLine[]>([]);
	let sources = $state.raw<LogSource[]>([]);
	let dropped = $state(0);
	let status = $state<'connecting' | 'streaming' | 'ended'>('connecting');
	let stick = $state(true);
	let restarts = $state(0);
	let scroller = $state<HTMLDivElement>();
	let search = $state<HTMLInputElement>();

	const labels = $derived(sourceLabels(sources));
	const showSource = $derived(sources.length > 1);
	const pods = $derived([...new Set(sources.map((s) => s.pod))]);
	/** A color per pod, so that a pod keeps its color in the picker and on its lines. */
	const colorOf = (source: number) => COLORS[Math.max(0, pods.indexOf(sources[source]?.pod)) % COLORS.length];
	const query = $derived(parseQuery(filter));
	const matcher = $derived(lineMatcher(query, sources));
	// Fold first, so that only lines that really came in a row count as repeats; a row keeps the level of its first line.
	const rows = $derived(
		foldRows(
			lines.filter((l) => l.source === null || !hiddenPods.has(sources[l.source]?.pod)),
			fold
		).filter((r) => atLevel(r.line, minLevel))
	);
	const isMatch = (row: Row) => row.line.kind === 'line' && (matcher(row.line) || row.more.some(matcher));
	const visible = $derived(filter && onlyMatches ? rows.filter((r) => r.line.kind !== 'line' || isMatch(r)) : rows);
	/** Ids of the rows that match the search, in order. */
	const hits = $derived(filter ? visible.filter(isMatch).map((r) => r.line.id) : []);
	const hitSet = $derived(new Set(hits));
	const shownCount = $derived(visible.reduce((n, r) => n + (r.line.kind === 'line' ? r.count + r.more.length : 0), 0));
	const total = $derived(lines.reduce((n, l) => n + (l.kind === 'line' ? 1 : 0), 0));
	const names = $derived(fieldNames(lines.slice(-2000)));
	const columns = $derived(picked ?? defaultColumns(names));
	/** Rollouts and other changes Kubri recorded, as markers in the rate strip. */
	let changes = $state.raw<{ at: number; text: string }[]>([]);
	$effect(() => {
		const t = target;
		if (!t.namespace || (t.context && t.context !== app.cluster?.context)) return;
		const kind = app.resources.find((r) => r.plural === t.resource.plural && r.group === t.resource.group)?.kind;
		let cancelled = false;
		const load = async () => {
			try {
				const page = await backend.history({
					namespace: t.namespace ?? undefined,
					// A pod's own changes are its workload's; a workload has its own.
					...(kind === 'Pod' ? {} : { kind, name: t.name }),
					since: Date.now() - 7 * 24 * 3600_000,
					changes: ['image', 'rollout', 'restarted', 'scaled', 'data'],
					limit: 100
				});
				if (!cancelled) changes = page.entries.map((e) => ({ at: e.at, text: `${e.kind} ${e.name}: ${e.summary}` }));
			} catch {
				// Without a change history there are no markers.
			}
		};
		untrack(load);
		const timer = setInterval(() => document.hidden || load(), 30_000);
		return () => {
			cancelled = true;
			clearInterval(timer);
		};
	});
	const buckets = $derived(rate(visible, 60, changes));

	/** Why the container shown does not run, from the pod's status: a container that never started has no logs to show. */
	const cause = $derived.by(() => {
		if (!pod || previous || loki || app.snapshot) return null;
		for (const name of container ? [container] : containers) {
			const found = containerCause(pod, name);
			if (found) {
				const s = [...(pod.status?.initContainerStatuses ?? []), ...(pod.status?.containerStatuses ?? [])].find((c: any) => c.name === name);
				return { container: name, cause: found, reason: s?.state?.waiting?.reason ?? s?.state?.terminated?.reason ?? null, ran: !!s?.lastState?.terminated };
			}
		}
		return null;
	});
	const neverStarted = $derived(cause !== null && !cause.ran && !lines.some((l) => l.kind !== 'error'));

	/** Which instance and stream the lines come from, so that a view never passes for another. */
	const streamState = $derived.by((): { tone: 'warn' | 'ok' | ''; text: string } => {
		if (neverStarted && cause) return { tone: 'warn', text: t('No logs: {container} never started.', { container: cause.container }) };
		if (loki) return { tone: '', text: lokiSource ? t('History from Loki at {source}. These lines do not update.', { source: lokiSource }) : t('Asking Loki…') };
		if (previous) return { tone: 'warn', text: t('Previous container instance, which ended. These lines do not update.') };
		if (app.snapshot) return { tone: '', text: t('The last lines saved in the snapshot. These lines do not update.') };
		if (status === 'connecting') return { tone: '', text: t('Connecting to the log stream…') };
		if (status === 'ended' && follow && cause) return { tone: 'warn', text: t('The stream ended because {container} stopped.', { container: cause.container }) };
		if (status === 'ended') return { tone: '', text: follow ? t('The stream ended. Reconnect to continue.') : t('Snapshot of the current instance.') };
		return { tone: 'ok', text: follow ? t('Streaming the current container instance.') : t('Snapshot of the current instance.') };
	});

	/** Colors lines by their level, when they name one; lines without one stay plain. */
	function tone(line: ViewLine): string {
		if (line.kind === 'error' || line.level === 'error') return 'bad';
		if (line.level === 'warn') return 'warn';
		if (line.level === 'debug') return 'dim';
		return '';
	}

	function toggle(id: number) {
		const next = new Set(open);
		if (!next.delete(id)) next.add(id);
		open = next;
	}

	/** Shows one pod alone with a double click, or every pod again. */
	function togglePod(pod: string, only = false) {
		if (only) hiddenPods = hiddenPods.size === pods.length - 1 && !hiddenPods.has(pod) ? new Set() : new Set(pods.filter((p) => p !== pod));
		else {
			const next = new Set(hiddenPods);
			if (!next.delete(pod)) next.add(pod);
			hiddenPods = next;
		}
	}

	function pick(column: string, on: boolean) {
		const now = columns.filter((c) => c !== column);
		picked = on ? [...now, column] : now;
	}

	$effect(() => {
		void restarts;
		const options: LogOptions = {
			container: container || null,
			follow,
			previous,
			...RANGES.get(range)?.options
		};
		lines = [];
		sources = [];
		hiddenPods = new Set();
		open = new Set();
		dropped = 0;
		stick = autoscroll;
		status = 'connecting';

		// Lines can arrive faster than the screen refreshes: render at most once per frame.
		let nextId = 0;
		let buffer: ViewLine[] = [];
		let frame = 0;
		const flush = () => {
			frame = 0;
			const all = appendInOrder(lines, buffer);
			buffer = [];
			const excess = all.length - MAX_LINES;
			if (excess > 0) dropped += excess;
			lines = excess > 0 ? all.slice(excess) : all;
		};
		const push = (line: Omit<ViewLine, 'id'>) => {
			buffer.push({ id: nextId++, ...line });
			if (!frame) frame = requestAnimationFrame(flush);
		};
		const onEvent = (event: LogEvent) => {
			switch (event.type) {
				case 'sources':
					sources = event.sources;
					status = 'streaming';
					break;
				case 'lines':
					for (const l of event.lines) push({ ...l, text: stripAnsi(l.text), kind: 'line' });
					break;
				case 'error':
					push({ source: event.source, text: event.message, kind: 'error' });
					break;
				case 'marker':
					push({ source: null, text: event.text, time: event.time ?? new Date().toISOString(), kind: 'marker' });
					break;
				case 'end':
					status = 'ended';
					break;
			}
		};
		if (loki) {
			let cancelled = false;
			const request = { target, query: ranLogql.trim() || null, container: container || null, seconds: lokiSeconds };
			backend.lokiLogs(request).then(
				(found) => {
					if (cancelled) return;
					sources = found.sources;
					lokiSource = found.source;
					if (!ranLogql) logql = found.query;
					if (found.limited && found.lines.length)
						push({
							source: null,
							text: t('Loki has more lines: these are the newest {n}. Narrow the range or the query for older ones.', { n: found.lines.length }),
							time: found.lines[0].time,
							kind: 'marker'
						});
					for (const l of found.lines) push({ ...l, text: stripAnsi(l.text), kind: 'line' });
					status = 'ended';
				},
				(err) => {
					if (cancelled) return;
					push({ source: null, text: errorMessage(err), kind: 'error' });
					status = 'ended';
				}
			);
			return () => {
				cancelled = true;
				cancelAnimationFrame(frame);
			};
		}
		const stop = backend.streamLogs(target, options, onEvent);
		return () => {
			stop();
			cancelAnimationFrame(frame);
		};
	});

	$effect(() => {
		void visible;
		if (stick) tick().then(() => scroller && (scroller.scrollTop = scroller.scrollHeight));
	});

	function onscroll() {
		if (!scroller) return;
		stick = scroller.scrollTop + scroller.clientHeight >= scroller.scrollHeight - 24;
		autoscroll = stick;
	}

	function jumpToEnd() {
		stick = autoscroll = true;
		if (scroller) scroller.scrollTop = scroller.scrollHeight;
	}

	/** Stops following the latest line, or jumps back to it. */
	function toggleAutoscroll() {
		if (stick) stick = autoscroll = false;
		else jumpToEnd();
	}

	/** Exactly what is shown: repeats as a count, folded lines as how many there are. */
	function shownLines() {
		return rowText(visible, {
			label: (l) => (showSource && l.source !== null ? `[${labels[l.source]}] ` : ''),
			time: timestamps,
			open
		});
	}

	/** Brings a row into view, without autoscroll pulling it away again. */
	function reveal(id: number) {
		stick = autoscroll = false;
		tick().then(() =>
			scroller?.querySelector(`[data-id="${id}"]`)?.scrollIntoView({ block: 'center', behavior: 'instant' })
		);
	}

	/** Steps to the next or previous match; n and N. */
	function step(by: 1 | -1) {
		if (!hits.length) {
			if (filter) app.toast(t('No lines match'), 'bad');
			return;
		}
		const n = hits.length;
		current = current < 0 || current >= n ? (by === 1 ? 0 : n - 1) : (current + by + n) % n;
		reveal(hits[current]);
	}

	$effect(() => {
		void filter;
		current = -1;
	});

	function save() {
		saveFile(`${target.name}${container ? `-${container}` : ''}.log`, shownLines().join('\n') + '\n');
	}

	function copy() {
		copyLines(shownLines(), (n) => t('Copied {n} lines', { n }));
	}

	/** Asks the model about the selected lines, or else the last 200 shown. */
	function explain() {
		const lines = selectedText(scroller) ?? shownLines().slice(-200).join('\n');
		if (!lines.trim()) {
			app.toast(t('There are no log lines to explain yet'), 'bad');
			return;
		}
		app.explaining = { kind: 'logs', target, container: container || null, lines };
	}

	/** A line across the log, to tell what came after it; k9s `m`. */
	function mark() {
		lines = [...lines, { id: --marks, source: null, text: '', kind: 'mark' }];
		jumpToEnd();
	}

	/** Enter steps through matches; Esc clears the search, then leaves it for the log keys. */
	function onsearchkey(e: KeyboardEvent) {
		if (e.key === 'Enter') {
			e.preventDefault();
			step(e.shiftKey ? -1 : 1);
			return;
		}
		if (e.key !== 'Escape') return;
		e.stopPropagation();
		if (filter) filter = '';
		else search?.blur();
	}

	/** The k9s log keys, before the list below gets them. */
	function onkeydown(e: KeyboardEvent) {
		if (!active || app.overlayOpen || forControl(e) || e.altKey) return;
		const ctrl = e.ctrlKey || e.metaKey;
		const handled = (() => {
			if (ctrl) {
				if (e.key.toLowerCase() !== 's') return false;
				save();
				return true;
			}
			const key = RANGE_KEYS[Number(e.key)];
			if (/^[0-6]$/.test(e.key) && key) {
				range = key;
				return true;
			}
			switch (e.key) {
				case '/':
					search?.focus();
					return true;
				case 't':
					timestamps = !timestamps;
					return true;
				case 'w':
					wrap = !wrap;
					return true;
				case 'c':
					copy();
					return true;
				case 'C':
					lines = [];
					return true;
				case 's':
					toggleAutoscroll();
					app.toast(stick ? t('Autoscroll on') : t('Autoscroll off'));
					return true;
				case 'm':
					mark();
					return true;
				case 'x':
					explain();
					return true;
				case 'n':
				case 'N':
					if (!filter) return false;
					step(e.key === 'n' ? 1 : -1);
					return true;
			}
			return false;
		})();
		if (handled) {
			e.preventDefault();
			e.stopPropagation();
		}
	}
</script>

<svelte:window
	onkeydowncapture={onkeydown}
	onclick={(e) => {
		// A menu closes when the click is elsewhere.
		for (const d of document.querySelectorAll<HTMLDetailsElement>('details.columns[open]')) {
			if (!d.contains(e.target as Node)) d.open = false;
		}
	}}
/>

{#snippet textLine(line: ViewLine, extra = '')}
	{#if timestamps}<span class="time" title={line.time}>{shortTime(line.time) || ' '.repeat(12)}</span>{/if}{#if showSource && line.source !== null}<span
			class="source"
			style:color={colorOf(line.source)}>{labels[line.source]}</span
		>{/if}<span class={extra}>{line.text}</span>
{/snippet}

<div class="logs">
	<div class="toolbar">
		{#if containers.length > 1}
			<Select bind:value={container} options={[{ value: '', label: t('All containers') }, ...plainOptions(containers)]} title={t('Container')} aria-label={t('Container')} compact />
		{:else if containers.length === 1}
			<span class="container" title={t('Container')}>{containers[0]}</span>
		{/if}
		<Select
			value={loki ? 'loki' : previous ? 'previous' : 'current'}
			options={[
				{ value: 'current', label: t('Current instance') },
				{ value: 'previous', label: t('Previous · ended') },
				...(app.integrations.has('loki') ? [{ value: 'loki', label: t('Loki history') }] : [])
			]}
			onchange={(value) => {
				loki = value === 'loki';
				previous = value === 'previous';
			}}
			title={t('The running container, the one before it, which ended, or the history Loki kept')}
			aria-label={t('Log source')}
			compact
		/>
		{#if loki}
			<Select
				bind:value={lokiSeconds}
				options={LOKI_RANGES.map(([value, label]) => ({ value, label }))}
				title={t('How far back to search Loki')}
				aria-label={t('Time range')}
				compact
			/>
		{:else}
			<Select
				bind:value={range}
				options={[...RANGES].map(([value, r]) => ({ value, label: r.label }))}
				title={t('How far back to start (0–6)')}
				aria-label={t('Time range')}
				compact
			/>
		{/if}
		<Select bind:value={minLevel} options={LEVELS} title={t('Only lines at this level or above')} aria-label={t('Level')} compact />
		<label class="search" title={t('Search (/). Enter and n go to the next match, Shift+Enter and N to the previous.')}>
			<Icon name="search" size={15} />
			<input
				bind:this={search}
				bind:value={filter}
				onkeydown={onsearchkey}
				aria-label={t('Search log lines')}
				placeholder={t('Search… /regex/ or fields: level>=warn status>=500')}
			/>
			{#if filter}
				<span class="count" aria-live="polite"
					>{hits.length ? (current >= 0 ? t('{current} of {n}', { current: current + 1, n: hits.length }) : hits.length) : t('No matches')}</span
				>
				<button class="step" onclick={() => step(-1)} title={t('Previous match (N)')} aria-label={t('Previous match')}
					><Icon name="up" size={14} /></button
				>
				<button class="step" onclick={() => step(1)} title={t('Next match (n)')} aria-label={t('Next match')}
					><Icon name="chevron" size={14} /></button
				>
			{/if}
		</label>
		{#if filter}
			<label title={t('Hide the lines that do not match, or keep them and highlight the matches')}
				><input type="checkbox" bind:checked={onlyMatches} /> {t('Only matches')}</label
			>
		{/if}
		{#if !loki}
			<button class="toggle" class:on={follow} aria-pressed={follow} onclick={() => (follow = !follow)} title={t('Keep reading new lines as they are written')}>
				{t('Follow')}
			</button>
		{/if}
		<details class="columns view" {@attach dropdown}>
			<summary>{t('View')}</summary>
			<div class="menu">
				<label title="t"><input type="checkbox" bind:checked={timestamps} /> {t('Timestamps')}</label>
				<label title="w"><input type="checkbox" bind:checked={wrap} /> {t('Wrap lines')}</label>
				<label title={t('Count repeated lines and fold stack traces under their first line')}
					><input type="checkbox" bind:checked={fold} /> {t('Fold repeats')}</label
				>
				{#if names.length}
					<label title={t('Show JSON and logfmt lines as a table of their fields')}
						><input type="checkbox" bind:checked={table} /> {t('Table of fields')}</label
					>
				{/if}
				<label title={t('Keep the latest line in view (s)')}><input type="checkbox" checked={stick} onchange={toggleAutoscroll} /> {t('Autoscroll')}</label>
			</div>
		</details>
		<button class="export" onclick={save} title={t('Save the shown lines (Ctrl+S)')}><Icon name="download" size={15} /> {t('Export')}</button>
	</div>
	{#if loki}
		<form
			class="logql"
			onsubmit={(e) => {
				e.preventDefault();
				if (logql === ranLogql) restarts++;
				else ranLogql = logql;
			}}
		>
			<label for="logql">LogQL</label>
			<input
				id="logql"
				bind:value={logql}
				onkeydown={(e) => e.key === 'Escape' && e.currentTarget.blur()}
				spellcheck="false"
				autocomplete="off"
				placeholder={'{namespace="…", pod=~"…"}'}
			/>
			<button type="submit">{t('Run')}</button>
		</form>
	{/if}
	{#if pods.length > 1}
		<div class="pods" role="group" aria-label={t('Pods')}>
			<span class="muted">{t('Pods')}</span>
			{#each pods as pod (pod)}
				<button
					class="pod"
					aria-pressed={!hiddenPods.has(pod)}
					onclick={() => togglePod(pod)}
					ondblclick={() => togglePod(pod, true)}
					title={t('Show or hide {pod}; double-click to show it alone', { pod })}
				>
					<span class="dot" style:background={COLORS[pods.indexOf(pod) % COLORS.length]}></span>{pod}
				</button>
			{/each}
		</div>
	{/if}
	<div class="stream-state {streamState.tone}">
		<span class="state" role="status">
			<Icon name={streamState.tone === 'ok' ? 'event' : 'pending'} size={15} />
			{streamState.text}{stick ? '' : ` ${t('Autoscroll is paused.')}`}
		</span>
		<span class="buttons">
			{#if table && names.length}
				<details class="columns" {@attach dropdown}>
					<summary>{t('Columns')}</summary>
					<div class="menu">
						{#each names.slice(0, 40) as name (name)}
							<label
								><input type="checkbox" checked={columns.includes(name)} onchange={(e) => pick(name, e.currentTarget.checked)} />
								{name}</label
							>
						{/each}
						{#if picked}<button onclick={() => (picked = null)}>{t('Reset')}</button>{/if}
					</div>
				</details>
			{/if}
			<button onclick={mark} title={t('Draw a line to tell what comes after it (m)')}>{t('Mark')}</button>
			<button onclick={copy} title={t('Copy the shown lines (c)')}>{t('Copy')}</button>
			<!-- Pressing keeps the selection, which is what gets explained. -->
			<button onmousedown={(e) => e.preventDefault()} onclick={explain} title={t('Explain the selected lines, or the last 200 shown (x)')}>{t('Explain')}</button>
			<button onclick={() => (lines = [])} title={t('Clear the view (Shift+C)')}>{t('Clear')}</button>
		</span>
	</div>
	{#if buckets}
		<LogRate {buckets} onjump={(i) => visible[i] && reveal(visible[i].line.id)} />
	{/if}
	<div class="body" class:wrap bind:this={scroller} {onscroll}>
		{#if dropped}
			<div class="note">{plural(dropped, '{n} older line was dropped to keep the view fast.', '{n} older lines were dropped to keep the view fast.')}</div>
		{/if}
		{#if neverStarted && cause}
			<div class="cause">
				<CauseCard cause={cause.cause} target={target} object={pod!} container={cause.container} raw={cause.reason} />
			</div>
		{:else if table && names.length}
			<table>
				<thead>
					<tr>
						<th>{t('Time')}</th>
						<th>{t('Level')}</th>
						{#if showSource}<th>{t('Source')}</th>{/if}
						{#each columns as c (c)}<th>{c}</th>{/each}
					</tr>
				</thead>
				<tbody>
					{#each visible as row (row.line.id)}
						{@const line = row.line}
						{#if line.kind === 'mark'}
							<tr class="mark" data-id={line.id}><td colspan={columns.length + 3}></td></tr>
						{:else if line.kind === 'marker'}
							<tr class="marker" data-id={line.id}><td colspan={columns.length + 3}><span>{line.text}</span></td></tr>
						{:else}
							<tr
								class="{tone(line)} row"
								class:hit={!onlyMatches && hitSet.has(line.id)}
								class:current={hits[current] === line.id}
								data-id={line.id}
								onclick={() => window.getSelection()?.isCollapsed !== false && toggle(line.id)}
								title={t('Click to show the whole line')}
							>
								<td class="time" title={line.time}>{shortTime(line.time)}</td>
								<td>{line.level ?? ''}</td>
								{#if showSource}<td style:color={line.source !== null ? colorOf(line.source) : null}
										>{line.source !== null ? labels[line.source] : ''}</td
									>{/if}
								{#if line.fields}
									{#each columns as c (c)}<td>{cell(fieldValue(line.fields, c))}</td>{/each}
								{:else}
									<td colspan={columns.length}
										>{line.text}{#if row.count > 1}<span class="times">× {row.count}</span>{/if}{#if row.more.length}<span
											class="times">{plural(row.more.length, '+{n} line', '+{n} lines')}</span
										>{/if}</td
									>
								{/if}
							</tr>
							{#if open.has(line.id)}
								<tr class="detail"><td colspan={columns.length + 3}><pre>{[pretty(line), ...row.more.map((m) => m.text)].join('\n')}</pre></td></tr>
							{/if}
						{/if}
					{/each}
				</tbody>
			</table>
		{:else}
			{#each visible as row (row.line.id)}
				{@const line = row.line}
				{#if line.kind === 'mark'}
					<div class="line mark" data-id={line.id}></div>
				{:else if line.kind === 'marker'}
					<div class="marker" data-id={line.id}><span>{line.text}</span></div>
				{:else}
					<div
						class="line {tone(line)}"
						class:error={line.kind === 'error'}
						class:hit={!onlyMatches && hitSet.has(line.id)}
						class:current={hits[current] === line.id}
						data-id={line.id}
					>
						{@render textLine(line)}{#if row.count > 1}<span class="times" title={t('The same line {count} times in a row', { count: row.count })}
								>× {row.count}</span
							>{/if}{#if line.fields && line.text.trimStart().startsWith('{')}<button
								class="fold"
								onclick={() => toggle(line.id)}
								aria-expanded={open.has(line.id)}
								title={t('Show the fields of this line')}>{open.has(line.id) ? t('Hide fields') : '{…}'}</button
							>{/if}
					</div>
					{#if line.fields && open.has(line.id) && !row.more.length}
						<pre class="pretty">{pretty(line)}</pre>
					{/if}
					{#if row.more.length}
						{#if open.has(line.id)}
							{#each row.more as m (m.id)}
								<div class="line {tone(m)} more" class:hit={!onlyMatches && filter && matcher(m)}>{@render textLine(m)}</div>
							{/each}
						{/if}
						<button class="fold more-toggle" onclick={() => toggle(line.id)} aria-expanded={open.has(line.id)}>
							{open.has(line.id) ? t('Fold') : plural(row.more.length, '… {n} more line', '… {n} more lines')}
						</button>
					{/if}
				{/if}
			{/each}
		{/if}
		{#if neverStarted}
			<!-- The card says why. -->
		{:else if status === 'connecting'}
			<div class="note">{t('Connecting…')}</div>
		{:else if visible.length === 0}
			<div class="note">{filter || minLevel || hiddenPods.size ? t('No lines match.') : t('No log lines yet.')}</div>
		{/if}
		{#if status === 'ended' && cause && !neverStarted}
			<div class="cause">
				<CauseCard cause={cause.cause} target={target} object={pod!} container={cause.container} raw={cause.reason} onprevious={() => (previous = true)} />
				<button class="again" onclick={() => restarts++}>{t('Reconnect')}</button>
			</div>
		{:else if status === 'ended' && !neverStarted}
			<div class="note end">
				{follow && !previous && !loki ? t('The stream ended.') : t('End of log.')}
				<button onclick={() => restarts++}>{follow && !previous && !loki ? t('Reconnect') : t('Reload')}</button>
			</div>
		{/if}
	</div>
	<div class="foot">
		<span>{shownCount !== total ? plural(total, '{shown} of {n} line', '{shown} of {n} lines', { shown: shownCount }) : plural(shownCount, '{n} line', '{n} lines')}</span>
		<span
			>{loki ? t('Loki history') : previous ? t('Previous instance') : t('Current instance')} · {app.snapshot ? t('snapshot') : status === 'streaming' ? t('live') : status === 'ended' ? t('ended') : t('connecting')}</span
		>
	</div>

	{#if !stick}
		<button class="jump" onclick={jumpToEnd}>{t('↓ Latest')}</button>
	{/if}
</div>

<style>
	.cause {
		display: grid;
		justify-items: start;
		gap: 8px;
		padding: 16px;
	}
	.cause > :global(section) {
		justify-self: stretch;
		max-width: 720px;
	}
	.again {
		padding: 0;
		border: none;
		background: none;
		color: var(--accent-strong);
		font-family: var(--sans);
		font-size: 12.5px;
		font-weight: 600;
	}
	.container {
		padding: 3px 8px;
		border-radius: 5px;
		background: var(--surface-2);
		font-family: var(--mono);
		font-size: 12px;
		font-variant-ligatures: none;
	}
	/* Where the lines come from, and how they are shown. */
	.stream-state {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 6px 14px;
		padding: 6px 16px;
		border-top: 1px solid var(--border-subtle);
		border-bottom: 1px solid var(--border-subtle);
		background: var(--surface-2);
		font-size: 12px;
	}
	.state {
		display: flex;
		flex: 1 1 260px;
		align-items: center;
		gap: 8px;
		color: var(--text-muted);
	}
	.stream-state.warn {
		background: color-mix(in srgb, var(--warn) 12%, var(--surface-1));
	}
	.warn .state {
		color: var(--warn);
		font-weight: 600;
	}
	.ok .state {
		color: var(--ok);
	}
	.buttons {
		display: flex;
		gap: 6px;
	}
	.logs {
		position: relative;
		display: flex;
		flex-direction: column;
		min-height: 0;
	}
	.toolbar {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 8px 10px;
		padding: 8px 16px;
		font-size: 12.5px;
	}
	.logql {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 0 16px 8px;
		font-size: 12.5px;
	}
	.logql label {
		color: var(--text-muted);
	}
	.logql input {
		flex: 1;
		min-width: 0;
		padding: 5px 8px;
		font-family: var(--mono);
		font-size: 12px;
	}
	.search {
		flex: 1 1 220px;
		min-width: 160px;
		padding: 0 10px;
		border: 1px solid var(--border);
		border-radius: 6px;
		background: var(--surface-1);
	}
	.search:focus-within {
		border-color: var(--accent);
	}
	.search input {
		flex: 1;
		min-width: 0;
		padding: 5px 0;
		border: none;
		background: none;
		box-shadow: none;
		font-size: 12.5px;
	}
	.export {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		font-weight: 600;
	}
	label {
		display: flex;
		align-items: center;
		gap: 4px;
		color: var(--text-muted);
		white-space: nowrap;
	}
	.toolbar button,
	.buttons button,
	.end button {
		padding: 4px 10px;
		border: 1px solid var(--border);
		border-radius: 6px;
		background: var(--surface-1);
		font-size: 12px;
	}
	.body {
		flex: 1;
		min-height: 0;
		overflow: auto;
		padding: 8px 0 24px;
		background: var(--bg);
		font-family: var(--mono);
		font-size: 12.5px;
		line-height: 1.55;
	}
	.line {
		padding: 0 16px;
		white-space: pre;
	}
	.wrap .line {
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}
	.line:hover {
		background: var(--surface-2);
	}
	.line.bad {
		color: var(--bad);
	}
	.line.warn {
		color: var(--warn);
	}
	.line.mark {
		height: 0;
		margin: 6px 16px;
		padding: 0;
		border-top: 2px solid var(--accent);
	}
	.line.dim {
		color: var(--text-muted);
	}
	.line.hit,
	tr.hit {
		background: color-mix(in srgb, var(--warn) 14%, transparent);
	}
	.line.current,
	tr.current {
		background: color-mix(in srgb, var(--accent) 22%, transparent);
		box-shadow: inset 3px 0 0 var(--accent);
	}
	.line.more {
		padding-left: 32px;
	}
	.time {
		margin-right: 10px;
		color: var(--text-muted);
		user-select: none;
	}
	.times {
		margin-left: 10px;
		padding: 0 6px;
		border-radius: 999px;
		background: var(--surface-2);
		color: var(--text-muted);
		font-size: 11px;
		user-select: none;
	}
	.fold {
		margin-left: 10px;
		padding: 0 6px;
		border: 1px solid var(--border);
		border-radius: 4px;
		background: var(--surface-1);
		color: var(--text-muted);
		font-family: var(--mono);
		font-size: 11px;
	}
	.more-toggle {
		display: block;
		margin: 0 0 2px 32px;
	}
	.pretty {
		margin: 2px 16px 6px 32px;
		padding: 8px 10px;
		border-radius: 6px;
		background: var(--surface-2);
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}
	/* Kubri's own notes in the stream: a pod joined, a log ended. */
	.marker,
	tr.marker td {
		display: flex;
		align-items: center;
		gap: 10px;
		margin: 6px 16px;
		color: var(--accent);
		font-family: Inter, system-ui, sans-serif;
		font-size: 12px;
	}
	.marker::before,
	.marker::after {
		content: '';
		flex: 1;
		border-top: 1px dashed var(--accent);
	}
	.marker span {
		flex: none;
	}
	tr.marker td {
		display: table-cell;
		text-align: center;
	}
	tr.mark td {
		padding: 0;
		border-top: 2px solid var(--accent);
	}
	table {
		min-width: 100%;
		border-collapse: collapse;
	}
	th {
		position: sticky;
		top: -8px;
		z-index: 1;
		padding: 4px 10px;
		background: var(--surface-2);
		color: var(--text-muted);
		font-family: Inter, system-ui, sans-serif;
		font-size: 11.5px;
		font-weight: 600;
		text-align: left;
		white-space: nowrap;
	}
	td {
		max-width: 480px;
		padding: 1px 10px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		vertical-align: top;
	}
	.wrap td {
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}
	td:first-child,
	th:first-child {
		padding-left: 16px;
	}
	tr.row {
		cursor: pointer;
	}
	tr.row:hover {
		background: var(--surface-2);
	}
	tr.bad {
		color: var(--bad);
	}
	tr.warn {
		color: var(--warn);
	}
	tr.dim {
		color: var(--text-muted);
	}
	tr.detail pre {
		margin: 2px 0 6px;
		padding: 8px 10px;
		border-radius: 6px;
		background: var(--surface-2);
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}
	.pods {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 6px;
		padding: 0 16px 8px;
		font-size: 12px;
	}
	.muted {
		color: var(--text-muted);
	}
	.pod {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		padding: 2px 8px;
		border: 1px solid var(--border);
		border-radius: 999px;
		background: var(--surface-1);
		font-family: var(--mono);
		font-size: 11.5px;
	}
	.pod[aria-pressed='false'] {
		opacity: 0.45;
		text-decoration: line-through;
	}
	.dot {
		width: 8px;
		height: 8px;
		border-radius: 50%;
	}
	.count {
		flex: none;
		color: var(--text-muted);
		font-size: 11.5px;
		white-space: nowrap;
	}
	.step {
		display: inline-flex;
		padding: 2px !important;
		border: none !important;
		background: none !important;
		color: var(--text-muted);
	}
	.columns {
		position: relative;
	}
	.toggle.on {
		border-color: var(--accent);
		background: color-mix(in srgb, var(--accent) 12%, var(--surface-1));
		color: var(--accent);
		font-weight: 600;
	}
	.columns summary {
		padding: 4px 10px;
		border: 1px solid var(--border);
		border-radius: 6px;
		background: var(--surface-1);
		cursor: pointer;
		list-style: none;
	}
	.columns .menu {
		position: absolute;
		right: 0;
		z-index: 5;
		display: grid;
		gap: 4px;
		min-width: 200px;
		max-height: 320px;
		overflow: auto;
		margin-top: 4px;
		padding: 8px 10px;
		border: 1px solid var(--border);
		border-radius: 8px;
		background: var(--surface-1);
		box-shadow: var(--shadow);
		font-family: var(--mono);
	}
	.source {
		margin-right: 8px;
		font-weight: 600;
		user-select: none;
	}
	.note {
		padding: 6px 16px;
		color: var(--text-muted);
		font-family: Inter, system-ui, sans-serif;
	}
	.end {
		display: flex;
		align-items: center;
		gap: 10px;
	}
	.foot {
		display: flex;
		justify-content: space-between;
		gap: 12px;
		padding: 6px 16px;
		border-top: 1px solid var(--border-subtle);
		font-size: 11.5px;
		color: var(--text-muted);
	}
	.jump {
		position: absolute;
		right: 24px;
		bottom: 16px;
		padding: 6px 12px;
		border: 1px solid var(--accent);
		border-radius: 999px;
		background: var(--accent);
		color: var(--on-accent);
		box-shadow: var(--shadow);
		font-size: 12px;
	}
</style>
