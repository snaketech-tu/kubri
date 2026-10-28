<script lang="ts">
	import { tick, untrack } from 'svelte';
	import { backend, type LogEvent, type LogOptions, type LogSource, type ObjectRef } from '../api';
	import { copyLines, saveFile } from '../actions';
	import { forControl } from '../keys';
	import { logMatcher, logRanges, sourceLabels, stripAnsi } from '../pods';
	import { app } from '../state.svelte';

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
		container?: { name: string } | null;
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
		if (wanted) container = wanted.name;
	});

	let lines = $state.raw<Line[]>([]);
	let sources = $state.raw<LogSource[]>([]);
	let dropped = $state(0);
	let status = $state<'connecting' | 'streaming' | 'ended'>('connecting');
	let stick = $state(true);
	let restarts = $state(0);
	let scroller = $state<HTMLDivElement>();

	const labels = $derived(sourceLabels(sources));
	const showSource = $derived(sources.length > 1);
	const matches = $derived(logMatcher(filter));
	const visible = $derived(filter ? lines.filter((l) => l.error || l.mark || matches(l.text)) : lines);

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
					if (stick) stick = autoscroll = false;
					else jumpToEnd();
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
			<select bind:value={container} title="Container">
				<option value="">All containers</option>
				{#each containers as c (c)}<option value={c}>{c}</option>{/each}
			</select>
		{/if}
		<select bind:value={range} title="How far back to start (0–6)">
			{#each [...RANGES] as [key, r] (key)}<option value={key}>{r.label}</option>{/each}
		</select>
		<label><input type="checkbox" bind:checked={follow} /> Follow</label>
		<label title="t"><input type="checkbox" bind:checked={timestamps} /> Timestamps</label>
		<label title="Logs of the previous, crashed container"><input type="checkbox" bind:checked={previous} /> Previous</label>
		<label title="w"><input type="checkbox" bind:checked={wrap} /> Wrap</label>
		<input class="filter" bind:value={filter} placeholder="Filter…  /regex/" />
		<span class="spacer"></span>
		<button onclick={() => (lines = [])} title="Clear the view (Shift+C)">Clear</button>
		<button onclick={mark} title="Draw a line to tell what comes after it (m)">Mark</button>
		<button onclick={copy} title="Copy the shown lines (c)">Copy</button>
		<button onclick={save} title="Save the shown lines (Ctrl+S)">Save</button>
	</div>

	<div class="body" class:wrap bind:this={scroller} {onscroll}>
		{#if dropped}
			<div class="note">{dropped} older lines were dropped to keep the view fast.</div>
		{/if}
		{#each visible as line (line.id)}
			<div class="line" class:error={line.error} class:mark={line.mark}>
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
				{follow ? 'The stream ended.' : 'End of log.'}
				<button onclick={() => restarts++}>{follow ? 'Reconnect' : 'Reload'}</button>
			</div>
		{/if}
	</div>

	{#if !stick}
		<button class="jump" onclick={jumpToEnd}>↓ Latest</button>
	{/if}
</div>

<style>
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
		gap: 6px 12px;
		padding: 8px 16px;
		border-bottom: 1px solid var(--border-subtle);
		font-size: 12.5px;
	}
	.toolbar select,
	.toolbar input.filter {
		padding: 3px 8px;
		font-size: 12.5px;
	}
	.toolbar input.filter {
		width: 160px;
	}
	label {
		display: flex;
		align-items: center;
		gap: 4px;
		color: var(--text-muted);
		white-space: nowrap;
	}
	.spacer {
		flex: 1;
	}
	.toolbar button,
	.end button {
		padding: 3px 10px;
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
		font-size: 12px;
		line-height: 1.5;
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
	.line.error {
		color: var(--bad);
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
