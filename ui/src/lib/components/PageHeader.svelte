<script lang="ts" module>
	export type { Crumb } from './Crumbs.svelte';
</script>

<script lang="ts">
	import type { Snippet } from 'svelte';
	import Crumbs, { type Crumb } from './Crumbs.svelte';

	let {
		crumbs,
		prefix = 'namespace',
		title,
		count,
		subtitle,
		mono = false,
		children,
		tools
	}: {
		crumbs: Crumb[];
		/** Where the page sits: the context, and its namespace unless the page is not namespaced; none for pages across contexts. */
		prefix?: 'namespace' | 'context' | 'none';
		title: string;
		count?: number | null;
		subtitle?: string;
		/** Object names read better in the code font. */
		mono?: boolean;
		/** Inline after the title: tags, a status. */
		children?: Snippet;
		/** Right-hand buttons. */
		tools?: Snippet;
	} = $props();
</script>

<header>
	<Crumbs {crumbs} {prefix} />
	<div class="heading">
		<div class="text">
			<div class="title">
				<h1 class:mono>{title}</h1>
				{#if count != null}<span class="count">{count}</span>{/if}
				{@render children?.()}
			</div>
			{#if subtitle}<p class="subtitle">{subtitle}</p>{/if}
		</div>
		{#if tools}<div class="tools">{@render tools()}</div>{/if}
	</div>
</header>

<style>
	header {
		padding: 20px 0 16px;
	}
	.heading {
		display: flex;
		flex-wrap: wrap;
		align-items: flex-end;
		justify-content: space-between;
		gap: 10px 16px;
	}
	.text {
		min-width: 0;
	}
	.title {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 6px 10px;
		min-width: 0;
	}
	h1 {
		margin: 0;
		font-size: 26px;
		letter-spacing: -0.03em;
		overflow-wrap: anywhere;
	}
	h1.mono {
		font-family: var(--mono);
		font-size: 22px;
		letter-spacing: -0.01em;
	}
	.count {
		font-size: 18px;
		font-weight: 600;
		color: var(--text-muted);
	}
	.subtitle {
		margin: 4px 0 0;
		font-size: 13px;
		color: var(--text-muted);
	}
	.tools {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 8px;
	}
</style>
