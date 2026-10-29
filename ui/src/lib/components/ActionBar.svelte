<script lang="ts">
	import type { Snippet } from 'svelte';
	import { hintOf, type MenuAction } from '../menu';
	import { app } from '../state.svelte';

	let {
		actions,
		children,
		marked = false,
		hints = true,
		compact = false
	}: {
		actions: MenuAction[];
		/** What the actions work on, shown first. */
		children?: Snippet;
		/** For marked rows rather than the one under the cursor. */
		marked?: boolean;
		/** Shows the keys, where they work on what the bar does. */
		hints?: boolean;
		/** Without a frame, as in the details panel, whose header names the object. */
		compact?: boolean;
	} = $props();

	/** How many buttons the bar shows before the rest go under More. */
	const BUTTONS = 6;

	const buttons = $derived(
		actions
			.filter((a) => a.primary !== undefined && !a.danger)
			.sort((a, b) => a.primary! - b.primary!)
			.slice(0, BUTTONS)
	);
	const dangerous = $derived(actions.filter((a) => a.primary !== undefined && a.danger));
	const more = $derived(actions.filter((a) => !buttons.includes(a) && !dangerous.includes(a)));

	/** A click leaves the focus where it was, so the keys keep working on the list. */
	const keepFocus = (e: MouseEvent) => e.preventDefault();

	function openMore(e: MouseEvent) {
		const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
		app.menu = { actions: more, x: rect.left, y: rect.bottom + 4, hints };
	}
</script>

<div class="bar" class:marked class:compact role="toolbar" aria-label="Actions">
	{#if children}<span class="subject">{@render children()}</span>{/if}
	{#each buttons as action, i (i)}
		{@render button(action)}
	{/each}
	{#if more.length}
		<button onclick={openMore} onmousedown={keepFocus} aria-haspopup="menu">More <span class="chevron" aria-hidden="true">▾</span></button>
	{/if}
	<span class="spacer"></span>
	{#each dangerous as action, i (i)}
		{@render button(action)}
	{/each}
</div>

{#snippet button(action: MenuAction)}
	{@const hint = hints ? hintOf(actions, action) : null}
	<button
		class:danger={action.danger}
		class:refused={action.disabled}
		aria-disabled={action.disabled ? 'true' : undefined}
		onclick={() => action.run()}
		onmousedown={keepFocus}
		title={action.disabled ? `${action.disabled}. Click for details.` : hint ? `${action.label} (${hint})` : action.label}
	>
		{action.label}
		{#if hint}<kbd>{hint}</kbd>{/if}
	</button>
{/snippet}

<style>
	.bar {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 6px;
		margin: 0 20px 8px;
		padding: 6px;
		border: 1px solid var(--border);
		border-radius: 9px;
		background: var(--surface-1);
		container-type: inline-size;
	}
	.bar.compact {
		margin: 0;
		padding: 0 16px 10px 20px;
		border: none;
		background: none;
	}
	.bar.marked {
		border-color: color-mix(in srgb, var(--accent) 45%, transparent);
		background: color-mix(in srgb, var(--accent) 8%, var(--surface-1));
	}
	.subject {
		max-width: 320px;
		padding: 0 8px 0 4px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		color: var(--text-muted);
		font-family: var(--mono);
		font-size: 12.5px;
	}
	.marked .subject {
		color: var(--accent-strong);
		font-family: inherit;
		font-size: 13px;
		font-weight: 600;
	}
	.spacer {
		flex: 1;
	}
	button {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		height: 28px;
		padding: 0 10px;
		border: 1px solid var(--border);
		border-radius: 7px;
		background: var(--surface-2);
		font-size: 13px;
		white-space: nowrap;
	}
	button:hover {
		border-color: var(--accent);
	}
	button.danger {
		border-color: color-mix(in srgb, var(--bad) 40%, var(--border));
		color: var(--bad);
	}
	button.danger:hover {
		border-color: var(--bad);
		background: color-mix(in srgb, var(--bad) 10%, var(--surface-2));
	}
	/* Not allowed: still clickable, to say why. */
	button.refused {
		border-style: dashed;
		background: none;
		color: var(--text-muted);
		opacity: 0.75;
	}
	button.refused:hover {
		border-color: var(--border);
		background: none;
		opacity: 1;
	}
	.chevron {
		font-size: 11px;
		color: var(--text-muted);
	}
	kbd {
		padding: 0 4px;
		border: 1px solid var(--border);
		border-radius: 4px;
		background: var(--surface-1);
		font-family: var(--mono);
		font-size: 10.5px;
		color: var(--text-muted);
	}
	/* Next to the details panel the list is narrow: the keys go, the buttons stay on fewer lines. */
	@container (max-width: 720px) {
		kbd {
			display: none;
		}
		.subject {
			max-width: 180px;
		}
	}
</style>
