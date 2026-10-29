<script lang="ts">
	import { backend } from '../api';
	import { parseMarkdown, type Inline } from '../markdown';

	let { text }: { text: string } = $props();

	const blocks = $derived(parseMarkdown(text));

	function follow(e: MouseEvent, href: string) {
		// The desktop window does not open links itself.
		if (!backend.desktop) return;
		e.preventDefault();
		backend.openUrl(href);
	}
</script>

{#snippet line(parts: Inline[])}
	{#each parts as part, i (i)}
		{#if part.type === 'code'}<code>{part.text}</code>
		{:else if part.type === 'strong'}<strong>{part.text}</strong>
		{:else if part.type === 'em'}<em>{part.text}</em>
		{:else if part.type === 'link'}<a href={part.href} target="_blank" rel="noopener noreferrer" onclick={(e) => follow(e, part.href)}>{part.text}</a>
		{:else}{part.text}{/if}
	{/each}
{/snippet}

<div class="markdown">
	{#each blocks as block, i (i)}
		{#if block.type === 'heading'}
			<p class="heading level-{Math.min(block.level, 3)}">{@render line(block.inlines)}</p>
		{:else if block.type === 'paragraph'}
			<p>{@render line(block.inlines)}</p>
		{:else if block.type === 'list'}
			{#if block.ordered}
				<ol start={block.start}>
					{#each block.items as item, j (j)}<li class:nested={item.depth > 0}>{@render line(item.inlines)}</li>{/each}
				</ol>
			{:else}
				<ul>
					{#each block.items as item, j (j)}<li class:nested={item.depth > 0}>{@render line(item.inlines)}</li>{/each}
				</ul>
			{/if}
		{:else if block.type === 'code'}
			<pre><code>{block.text}</code></pre>
		{:else if block.type === 'quote'}
			<blockquote>{@render line(block.inlines)}</blockquote>
		{:else}
			<hr />
		{/if}
	{/each}
</div>

<style>
	.markdown {
		font-size: 13.5px;
		line-height: 1.55;
		overflow-wrap: anywhere;
	}
	p,
	ul,
	ol,
	pre,
	blockquote {
		margin: 0 0 10px;
	}
	.heading {
		margin: 16px 0 6px;
		font-weight: 650;
	}
	.heading:first-child {
		margin-top: 0;
	}
	.level-1 {
		font-size: 16px;
	}
	.level-2 {
		font-size: 15px;
	}
	.level-3 {
		font-size: 13.5px;
	}
	ul,
	ol {
		padding-left: 22px;
	}
	li {
		margin: 2px 0;
	}
	li.nested {
		margin-left: 18px;
		list-style-type: circle;
	}
	code {
		padding: 1px 4px;
		border-radius: 4px;
		background: var(--surface-2);
		font-family: var(--mono);
		font-size: 12.5px;
	}
	pre {
		padding: 10px 12px;
		overflow: auto;
		border-radius: 8px;
		background: var(--surface-2);
	}
	pre code {
		padding: 0;
		background: none;
		white-space: pre;
	}
	blockquote {
		padding-left: 10px;
		border-left: 3px solid var(--border);
		color: var(--text-muted);
	}
	hr {
		border: none;
		border-top: 1px solid var(--border);
	}
	a {
		color: var(--accent);
	}
</style>
