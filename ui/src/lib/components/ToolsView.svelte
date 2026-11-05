<script lang="ts">
	import { TOOLS } from '../nav';
	import { app } from '../state.svelte';
	import Icon from './Icon.svelte';
	import PageHeader from './PageHeader.svelte';

	const running = $derived(app.benchmarks.filter((b) => b.state === 'running').length);

	function status(id: string) {
		if (id === 'portforwards' && app.forwards.length) return `${app.forwards.length} running`;
		if (id === 'benchmarks' && running) return `${running} running`;
		return '';
	}
</script>

<section class="page">
	<PageHeader crumbs={['Tools']} prefix="context" title="Tools" subtitle="Everything beyond the lists. Each tool also opens from the command palette." />
	<ul class="tiles">
		{#each TOOLS as tool (tool.id)}
			<li>
				<button class="tile" onclick={() => app.showTool(tool.id)}>
					<span class="icon"><Icon name={tool.icon} size={20} /></span>
					<span class="text">
						<span class="name">
							{tool.label}
							{#if status(tool.id)}<span class="status">{status(tool.id)}</span>{/if}
						</span>
						<span class="detail">{tool.detail}</span>
					</span>
					<kbd>{tool.command}</kbd>
				</button>
			</li>
		{/each}
	</ul>
</section>

<style>
	.page {
		display: flex;
		flex-direction: column;
		min-height: 0;
		min-width: 0;
		padding: 0 24px 20px;
		overflow-y: auto;
	}
	.tiles {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(min(100%, 300px), 1fr));
		gap: 12px;
		margin: 0;
		padding: 0;
		list-style: none;
	}
	.tile {
		display: grid;
		grid-template-columns: auto minmax(0, 1fr) auto;
		align-items: start;
		gap: 12px;
		width: 100%;
		height: 100%;
		padding: 14px 16px;
		border: 1px solid var(--border);
		border-radius: 10px;
		background: var(--surface-1);
		text-align: left;
	}
	.tile:hover {
		border-color: color-mix(in srgb, var(--accent) 45%, var(--border));
		background: var(--surface-2);
	}
	.icon {
		display: grid;
		place-items: center;
		width: 36px;
		height: 36px;
		border-radius: 8px;
		background: color-mix(in srgb, var(--accent) 12%, var(--surface-1));
		color: var(--accent-strong);
	}
	.text {
		display: grid;
		gap: 3px;
		min-width: 0;
	}
	.name {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		gap: 8px;
		color: var(--text);
		font-size: 14px;
		font-weight: 650;
	}
	.status {
		color: var(--accent-strong);
		font-size: 12px;
		font-weight: 500;
	}
	.detail {
		color: var(--text-muted);
		font-size: 12.5px;
		line-height: 1.45;
	}
	kbd {
		padding: 1px 6px;
		border: 1px solid var(--border);
		border-radius: 4px;
		background: var(--surface-2);
		color: var(--text-muted);
		font-family: var(--mono);
		font-size: 11.5px;
		white-space: nowrap;
	}
</style>
