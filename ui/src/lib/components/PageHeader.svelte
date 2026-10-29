<script lang="ts">
	import type { Snippet } from 'svelte';
	import Icon from './Icon.svelte';

	/** A breadcrumb step; one with `onclick` leads back to that place. */
	export type Crumb = string | { label: string; onclick: () => void; title?: string };

	let {
		crumbs,
		title,
		count,
		subtitle,
		mono = false,
		children,
		tools
	}: {
		crumbs: Crumb[];
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
	<nav class="crumbs" aria-label="Breadcrumb">
		{#each crumbs as crumb, i (i)}
			{#if i}<Icon name="next" size={12} />{/if}
			{#if typeof crumb === 'string'}
				<span aria-current={i === crumbs.length - 1 ? 'page' : undefined}>{crumb}</span>
			{:else}
				<button onclick={crumb.onclick} title={crumb.title}>{crumb.label}</button>
			{/if}
		{/each}
	</nav>
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
	.crumbs {
		display: flex;
		align-items: center;
		gap: 6px;
		margin-bottom: 6px;
		font-size: 12px;
		color: var(--text-muted);
	}
	.crumbs button {
		padding: 0;
		border: none;
		background: none;
		color: inherit;
		font-size: inherit;
	}
	.crumbs button:hover {
		color: var(--accent-strong);
		text-decoration: underline;
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
