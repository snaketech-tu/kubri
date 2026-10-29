<script lang="ts">
	import { SvelteSet } from 'svelte/reactivity';
	import { stringify } from 'yaml';
	import { diffLines, fold } from '../diff';

	let {
		before,
		after,
		maxHeight = '360px'
	}: { before: unknown; after: unknown; maxHeight?: string } = $props();

	const lines = $derived(diffLines(stringify(before, { lineWidth: 0 }), stringify(after, { lineWidth: 0 })));
	const parts = $derived(fold(lines));
	const added = $derived(lines.filter((l) => l.kind === 'added').length);
	const removed = $derived(lines.filter((l) => l.kind === 'removed').length);
	/** Folds opened by a click, by position. */
	const opened = new SvelteSet<number>();

	/** The lines a fold hides: those between the parts around it. */
	function hidden(index: number) {
		const start = parts.slice(0, index).reduce((n, p) => n + (p.kind === 'fold' ? p.count : p.lines.length), 0);
		const part = parts[index];
		return part.kind === 'fold' ? lines.slice(start, start + part.count) : [];
	}
</script>

<div class="diff" role="region" aria-label="Changes">
	<div class="summary">
		{#if added || removed}
			<span class="added">+{added}</span> <span class="removed">−{removed}</span>
			<span class="muted">lines, as the API server would store it</span>
		{:else}
			<span class="muted">No changes: the API server would store the object as it is.</span>
		{/if}
	</div>
	<div class="lines" style:max-height={maxHeight}>
		{#each parts as part, i (i)}
			{#if part.kind === 'fold' && !opened.has(i)}
				<button class="fold" onclick={() => opened.add(i)}>⋯ {part.count} unchanged {part.count === 1 ? 'line' : 'lines'}</button>
			{:else}
				{#each part.kind === 'fold' ? hidden(i) : part.lines as line, j (j)}
					<div class="line {line.kind}">
						<span class="no">{line.old ?? ''}</span><span class="no">{line.new ?? ''}</span><span class="mark"
							>{line.kind === 'added' ? '+' : line.kind === 'removed' ? '−' : ' '}</span
						><span class="text">{line.text}</span>
					</div>
				{/each}
			{/if}
		{/each}
	</div>
</div>

<style>
	.diff {
		border: 1px solid var(--border);
		border-radius: 8px;
		overflow: hidden;
		background: var(--bg);
	}
	.summary {
		padding: 6px 10px;
		border-bottom: 1px solid var(--border);
		background: var(--surface-2);
		font-size: 12px;
	}
	.muted {
		color: var(--text-muted);
	}
	.added {
		color: var(--ok);
		font-weight: 600;
	}
	.removed {
		color: var(--bad);
		font-weight: 600;
	}
	.lines {
		overflow: auto;
		font-family: var(--mono);
		font-size: 12px;
		line-height: 1.55;
	}
	.line {
		display: grid;
		grid-template-columns: 3.2em 3.2em 1.4em max-content;
		white-space: pre;
	}
	.line.added {
		background: color-mix(in srgb, var(--ok) 13%, transparent);
	}
	.line.removed {
		background: color-mix(in srgb, var(--bad) 13%, transparent);
	}
	.no {
		padding-right: 6px;
		color: var(--text-muted);
		text-align: right;
		user-select: none;
	}
	.mark {
		color: var(--text-muted);
		text-align: center;
		user-select: none;
	}
	.fold {
		display: block;
		width: 100%;
		padding: 2px 10px;
		border: none;
		border-block: 1px dashed var(--border-subtle);
		background: var(--surface-1);
		color: var(--text-muted);
		font-family: inherit;
		font-size: 11.5px;
		text-align: left;
	}
	.fold:hover {
		color: var(--text);
	}
</style>
