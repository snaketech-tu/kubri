<script lang="ts">
	import type { Snippet } from 'svelte';
	import { hintOf, type MenuAction } from '../menu';
	import { t } from '../i18n';
	import { app } from '../state.svelte';

	let {
		actions,
		children,
		marked = false,
		hints = true,
		compact = false,
		max = 6
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
		/** How many buttons the bar shows before the rest go under More. */
		max?: number;
	} = $props();

	// The bar keeps to one line, so the list below it never moves: the buttons that do not fit
	// go under More. Hidden copies of every button tell how wide each one is.
	const GAP = 6;
	let bar = $state<HTMLDivElement>();
	let measure = $state<HTMLDivElement>();
	let width = $state(0);
	let fit = $state(Infinity);

	const primary = $derived(
		actions.filter((a) => a.primary !== undefined && !a.danger).sort((a, b) => a.primary! - b.primary!)
	);
	const buttons = $derived(primary.slice(0, Math.min(max, fit)));
	const dangerous = $derived(actions.filter((a) => a.primary !== undefined && a.danger));
	const more = $derived(actions.filter((a) => !buttons.includes(a) && !dangerous.includes(a)));

	$effect(() => {
		void primary;
		void dangerous;
		if (!bar || !measure || !width) return;
		const widths = [...measure.children].map((c) => (c as HTMLElement).offsetWidth);
		fit = fitting(widths, primary.length, dangerous.length, actions.length > primary.length + dangerous.length);
	});

	/** How many of the first buttons fit beside the subject, More and the dangerous ones. */
	function fitting(widths: number[], count: number, danger: number, extra: boolean): number {
		const style = getComputedStyle(bar!);
		const room = bar!.clientWidth - parseFloat(style.paddingLeft) - parseFloat(style.paddingRight);
		// The subject gives way to the buttons down to a short name.
		const subject = bar!.querySelector<HTMLElement>('.subject');
		const named = subject ? Math.min(subject.scrollWidth, 140) + GAP : 0;
		const sum = (from: number, to: number) => widths.slice(from, to).reduce((a, w) => a + w + GAP, 0);
		// The spacer before the dangerous buttons takes a gap of its own; widths are rounded.
		const fixed = named + GAP + sum(count, count + danger) + 2;
		const moreWidth = widths[count + danger] + GAP;
		for (let n = Math.min(count, max); n > 0; n--) {
			const needsMore = extra || n < count;
			if (fixed + sum(0, n) + (needsMore ? moreWidth : 0) <= room + GAP) return n;
		}
		return 0;
	}

	/** A click leaves the focus where it was, so the keys keep working on the list. */
	const keepFocus = (e: MouseEvent) => e.preventDefault();

	function openMore(e: MouseEvent) {
		const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
		app.menu = { actions: more, x: rect.left, y: rect.bottom + 4, hints };
	}
</script>

<div class="bar" class:marked class:compact role="toolbar" aria-label={t('Actions')} bind:this={bar} bind:clientWidth={width}>
	{#if children}<span class="subject">{@render children()}</span>{/if}
	{#each buttons as action, i (i)}
		{@render button(action)}
	{/each}
	{#if more.length}
		<button onclick={openMore} onmousedown={keepFocus} aria-haspopup="menu">{t('More')} <span class="chevron" aria-hidden="true">▾</span></button>
	{/if}
	<span class="spacer"></span>
	{#each dangerous as action, i (i)}
		{@render button(action)}
	{/each}
	<div class="measure" bind:this={measure} aria-hidden="true" inert>
		{#each [...primary, ...dangerous] as action, i (i)}
			{@render button(action)}
		{/each}
		<button>{t('More')} <span class="chevron">▾</span></button>
	</div>
</div>

{#snippet button(action: MenuAction)}
	{@const hint = hints ? hintOf(actions, action) : null}
	<button
		class:danger={action.danger}
		class:refused={action.disabled}
		aria-disabled={action.disabled ? 'true' : undefined}
		onclick={() => action.run()}
		onmousedown={keepFocus}
		title={action.disabled ? t('{reason}. Click for details.', { reason: action.disabled }) : hint ? `${action.label} (${hint})` : action.label}
	>
		{action.label}
		{#if hint}<kbd>{hint}</kbd>{/if}
	</button>
{/snippet}

<style>
	.bar {
		position: relative;
		display: flex;
		align-items: center;
		gap: 6px;
		min-height: 42px;
		margin: 0 20px 8px;
		padding: 6px;
		border: 1px solid var(--border);
		border-radius: 9px;
		background: var(--surface-1);
		container-type: inline-size;
	}
	.bar.compact {
		min-height: 38px;
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
		flex: 0 1 auto;
		min-width: 0;
		height: 28px;
		max-width: 320px;
		line-height: 28px;
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
	/* Out of sight and out of the way, at the width each button would have. */
	.measure {
		position: absolute;
		top: 0;
		left: 0;
		display: flex;
		gap: 6px;
		width: 0;
		height: 0;
		overflow: hidden;
		visibility: hidden;
		pointer-events: none;
	}
	.measure button {
		flex: none;
	}
	.spacer {
		flex: 1;
	}
	button {
		flex: none;
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
