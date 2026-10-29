<script lang="ts">
	import { tick, untrack } from 'svelte';
	import { backend, type LogEvent, type LogOptions, type LogSource, type ObjectRef } from '../api';
	import { copyLines, saveFile } from '../actions';
	import { forControl } from '../keys';
	import { logMatcher, logRanges, sourceLabels, stripAnsi } from '../pods';
	import { app } from '../state.svelte';
	import Icon from './Icon.svelte';

	let {
		target,
		containers,
		previous: startPrevious = false,
		container: wanted = null,
		active = false
	}: {
		target: ObjectRef;
		containers: string[];
		previous?: boolean;
		/** A container to show, asked for from Describe; a new object each time. */
		container?: { name: string; previous?: boolean } | null;
		/** The logs are on screen and take the k9s log keys. */
		active?: boolean;
	} = $props();

	/** Older lines are dropped beyond this, to keep the view fast. */
	const MAX_LINES = 10_000;
	const COLORS = ['#3b82f6', '#10b981', '#f59e0b', '#ec4899', '#8b5cf6', '#06b6d4', '#ef4444', '#84cc16'];
	// k9s `logger` settings: lines to start with, how far back, wrap, timestamps, autoscroll.
	const logger = untrack(() => app.k9s?.logger);
	const { ranges: RANGES, start: startRange } = logRanges(logger?.tail ?? 100, logger?.sinceSeconds ?? -1);
	/** The ranges k9s puts on 0 to 6: tail, head (here everything), then 1m to 1h. */
	const RANGE_KEYS = ['tail', 'all', '1m', '5m', '15m', '30m', '1h'];

	interface Line {
		id: number;
		source: number | null;
		text: string;
		error?: boolean;
		/** A line drawn with `m`. */
		mark?: boolean;
	}

	let container = $state(untrack(() => wanted?.name ?? ''));
	let follow = $state(true);
	// Only the starting value; the checkbox owns it afterwards.
	let previous = $state(untrack(() => startPrevious));
	let timestamps = $state(logger?.showTime ?? false);
	let wrap = $state(logger?.textWrap ?? false);
	let range = $state(startRange);
	let filter = $state('');
	/** Keeps the newest line in view; k9s `s`. */
	let autoscroll = !logger?.disableAutoscroll;
	/** Ids of mark lines count down, apart from the log's own. */
	let marks = 0;

	$effect(() => {
		if (!wanted) return;
		container = wanted.name;
		if (wanted.previous !== undefined) previous = wanted.previous;
	});

	let lines = $state.raw<Line[]>([]);
	let sources = $state.raw<LogSource[]>([]);
	let dropped = $state(0);
	let status = $state<'connecting' | 'streaming' | 'ended'>('connecting');
	let stick = $state(true);
	let restarts = $state(0);
	let scroller = $state<HTMLDivElement>();
	let search = $state<HTMLInputElement>();

	const labels = $derived(sourceLabels(sources));
	const showSource = $derived(sources.length > 1);
	const matches = $derived(logMatcher(filter));
	const visible = $derived(filter ? lines.filter((l) => l.error || l.mark || matches(l.text)) : lines);

	/** Which instance and stream the lines come from, so that a view never passes for another. */
	const streamState = $derived.by((): { tone: 'warn' | 'ok' | ''; text: string } => {
		if (previous) return { tone: 'warn', text: 'Previous container instance, which ended. These lines do not update.' };
		if (status === 'connecting') return { tone: '', text: 'Connecting to the log stream…' };
		if (status === 'ended') return { tone: '', text: follow ? 'The stream ended. Reconnect to continue.' : 'Snapshot of the current instance.' };
		return { tone: 'ok', text: follow ? 'Streaming the current container instance.' : 'Snapshot of the current instance.' };
	});

	/** Colors lines by their level, when they name one; lines without one stay plain. */
	function level(text: string): 'bad' | 'warn' | '' {
		if (/\b(ERROR|FATAL|PANIC|CRITICAL|error|fatal|panic)\b/.test(text)) return 'bad';
		if (/\b(WARN|WARNING|warn|warning)\b/.test(text)) return 'warn';
		return '';
	}

	$effect(() => {
		void restarts;
		const options: LogOptions = {
			container: container || null,
			follow,
			previous,
			timestamps,
			...RANGES.get(range)?.options
		};
		lines = [];
		sources = [];
		dropped = 0;
		stick = autoscroll;
		status = 'connecting';

		// Lines can arrive faster than the screen refreshes: render at most once per frame.
		let nextId = 0;
		let buffer: Line[] = [];
		let frame = 0;
		const flush = () => {
			frame = 0;
			const all = lines.concat(buffer);
			buffer = [];
			const excess = all.length - MAX_LINES;
			if (excess > 0) dropped += excess;
			lines = excess > 0 ? all.slice(excess) : all;
		};
		const push = (line: Omit<Line, 'id'>) => {
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
					for (const l of event.lines) push({ source: l.source, text: stripAnsi(l.text) });
					break;
				case 'error':
					push({ source: event.source, text: event.message, error: true });
					break;
				case 'end':
					status = 'ended';
					break;
			}
		};
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

	function shownLines() {
		return visible.map((l) => (showSource && l.source !== null ? `[${labels[l.source]}] ` : '') + l.text);
	}

	function save() {
		saveFile(`${target.name}${container ? `-${container}` : ''}.log`, shownLines().join('\n') + '\n');
	}

	function copy() {
		copyLines(shownLines(), ['line', 'lines']);
	}

	/** A line across the log, to tell what came after it; k9s `m`. */
	function mark() {
		lines = [...lines, { id: --marks, source: null, text: '', mark: true }];
		jumpToEnd();
	}

	/** Esc clears the search, then leaves it for the log keys. */
	function onsearchkey(e: KeyboardEvent) {
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
					app.toast(stick ? 'Autoscroll on' : 'Autoscroll off');
					return true;
				case 'm':
					mark();
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

<svelte:window onkeydowncapture={onkeydown} />

<div class="logs">
	<div class="toolbar">
		{#if containers.length > 1}
			<select bind:value={container} title="Container" aria-label="Container">
				<option value="">All containers</option>
				{#each containers as c (c)}<option value={c}>{c}</option>{/each}
			</select>
		{:else if containers.length === 1}
			<span class="container" title="Container">{containers[0]}</span>
		{/if}
		<select
			value={previous ? 'previous' : 'current'}
			onchange={(e) => (previous = e.currentTarget.value === 'previous')}
			title="The running container, or the one before it, which ended"
			aria-label="Container instance"
		>
			<option value="current">Current instance</option>
			<option value="previous">Previous · ended</option>
		</select>
		<select bind:value={range} title="How far back to start (0–6)" aria-label="Time range">
			{#each [...RANGES] as [key, r] (key)}<option value={key}>{r.label}</option>{/each}
		</select>
		<label class="search" title="Search (/)">
			<Icon name="search" size={15} />
			<input
				bind:this={search}
				bind:value={filter}
				onkeydown={onsearchkey}
				aria-label="Filter log lines"
				placeholder="Search these logs… /regex/"
			/>
		</label>
		<button class="export" onclick={save} title="Save the shown lines (Ctrl+S)"><Icon name="download" size={15} /> Export</button>
	</div>
	<div class="stream-state {streamState.tone}">
		<span class="state" role="status">
			<Icon name={streamState.tone === 'ok' ? 'event' : 'pending'} size={15} />
			{streamState.text}{stick ? '' : ' Autoscroll is paused.'}
		</span>
		<label><input type="checkbox" bind:checked={follow} /> Follow</label>
		<label title="t"><input type="checkbox" bind:checked={timestamps} /> Timestamps</label>
		<label title="w"><input type="checkbox" bind:checked={wrap} /> Wrap</label>
		<label title="Keep the latest line in view (s)"><input type="checkbox" checked={stick} onchange={toggleAutoscroll} /> Autoscroll</label>
		<span class="buttons">
			<button onclick={mark} title="Draw a line to tell what comes after it (m)">Mark</button>
			<button onclick={copy} title="Copy the shown lines (c)">Copy</button>
			<button onclick={() => (lines = [])} title="Clear the view (Shift+C)">Clear</button>
		</span>
	</div>
	<div class="body" class:wrap bind:this={scroller} {onscroll}>
		{#if dropped}
			<div class="note">{dropped} older lines were dropped to keep the view fast.</div>
		{/if}
		{#each visible as line (line.id)}
			<div class="line {line.error ? 'bad' : level(line.text)}" class:error={line.error} class:mark={line.mark}>
				{#if showSource && line.source !== null}<span class="source" style:color={COLORS[line.source % COLORS.length]}
						>{labels[line.source]}</span
					>{/if}{line.text}
			</div>
		{/each}
		{#if status === 'connecting'}
			<div class="note">Connecting…</div>
		{:else if visible.length === 0}
			<div class="note">{filter ? 'No lines match the filter.' : 'No log lines yet.'}</div>
		{/if}
		{#if status === 'ended'}
			<div class="note end">
				{follow && !previous ? 'The stream ended.' : 'End of log.'}
				<button onclick={() => restarts++}>{follow && !previous ? 'Reconnect' : 'Reload'}</button>
			</div>
		{/if}
	</div>
	<div class="foot">
		<span>{visible.length} {visible.length === 1 ? 'line' : 'lines'}{filter ? ` of ${lines.length}` : ''}</span>
		<span>{previous ? 'Previous instance' : 'Current instance'} · {status === 'streaming' ? 'live' : status}</span>
	</div>

	{#if !stick}
		<button class="jump" onclick={jumpToEnd}>↓ Latest</button>
	{/if}
</div>

<style>
	.container {
		padding: 3px 8px;
		border-radius: 5px;
		background: var(--surface-2);
		font-family: var(--mono);
		font-size: 12px;
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
	.toolbar select {
		padding: 5px 8px;
		font-size: 12.5px;
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
