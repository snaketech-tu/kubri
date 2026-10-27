<script lang="ts">
	import { tick, untrack } from 'svelte';
	import { backend, type LogEvent, type LogOptions, type LogSource, type ObjectRef } from '../api';
	import { logMatcher, sourceLabels, stripAnsi } from '../pods';

	let { target, containers, previous: startPrevious = false }: {
		target: ObjectRef;
		containers: string[];
		previous?: boolean;
	} = $props();

	/** Older lines are dropped beyond this, to keep the view fast. */
	const MAX_LINES = 10_000;
	const COLORS = ['#3b82f6', '#10b981', '#f59e0b', '#ec4899', '#8b5cf6', '#06b6d4', '#ef4444', '#84cc16'];
	const RANGES: Record<string, { label: string; options: Pick<LogOptions, 'tailLines' | 'sinceSeconds'> }> = {
		'100': { label: 'Last 100 lines', options: { tailLines: 100 } },
		'500': { label: 'Last 500 lines', options: { tailLines: 500 } },
		'2000': { label: 'Last 2000 lines', options: { tailLines: 2000 } },
		'5m': { label: 'Last 5 minutes', options: { sinceSeconds: 300 } },
		'1h': { label: 'Last hour', options: { sinceSeconds: 3600 } },
		all: { label: 'Everything', options: {} }
	};

	interface Line {
		id: number;
		source: number | null;
		text: string;
		error?: boolean;
	}

	let container = $state('');
	let follow = $state(true);
	// Only the starting value; the checkbox owns it afterwards.
	let previous = $state(untrack(() => startPrevious));
	let timestamps = $state(false);
	let wrap = $state(true);
	let range = $state('500');
	let filter = $state('');

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
	const visible = $derived(filter ? lines.filter((l) => l.error || matches(l.text)) : lines);

	$effect(() => {
		void restarts;
		const options: LogOptions = {
			container: container || null,
			follow,
			previous,
			timestamps,
			...RANGES[range].options
		};
		lines = [];
		sources = [];
		dropped = 0;
		stick = true;
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
	}

	function jumpToEnd() {
		stick = true;
		if (scroller) scroller.scrollTop = scroller.scrollHeight;
	}

	function asText() {
		return visible.map((l) => (showSource && l.source !== null ? `[${labels[l.source]}] ` : '') + l.text).join('\n');
	}

	function download() {
		const url = URL.createObjectURL(new Blob([asText() + '\n'], { type: 'text/plain' }));
		const a = document.createElement('a');
		a.href = url;
		a.download = `${target.name}${container ? `-${container}` : ''}.log`;
		a.click();
		setTimeout(() => URL.revokeObjectURL(url), 1000);
	}
</script>

<div class="logs">
	<div class="toolbar">
		{#if containers.length > 1}
			<select bind:value={container} title="Container">
				<option value="">All containers</option>
				{#each containers as c (c)}<option value={c}>{c}</option>{/each}
			</select>
		{/if}
		<select bind:value={range} title="How far back to start">
			{#each Object.entries(RANGES) as [key, r] (key)}<option value={key}>{r.label}</option>{/each}
		</select>
		<label><input type="checkbox" bind:checked={follow} /> Follow</label>
		<label><input type="checkbox" bind:checked={timestamps} /> Timestamps</label>
		<label title="Logs of the previous, crashed container"><input type="checkbox" bind:checked={previous} /> Previous</label>
		<label><input type="checkbox" bind:checked={wrap} /> Wrap</label>
		<input class="filter" bind:value={filter} placeholder="Filter…  /regex/" />
		<span class="spacer"></span>
		<button onclick={() => (lines = [])} title="Clear the view">Clear</button>
		<button onclick={() => navigator.clipboard.writeText(asText())} title="Copy the shown lines">Copy</button>
		<button onclick={download} title="Save the shown lines">Save</button>
	</div>

	<div class="body" class:wrap bind:this={scroller} {onscroll}>
		{#if dropped}
			<div class="note">{dropped} older lines were dropped to keep the view fast.</div>
		{/if}
		{#each visible as line (line.id)}
			<div class="line" class:error={line.error}>
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
		color: white;
		box-shadow: var(--shadow);
		font-size: 12px;
	}
</style>
