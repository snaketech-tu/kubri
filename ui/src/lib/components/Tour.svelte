<script lang="ts">
	import { app } from '../state.svelte';

	/** The steps; each points at what it explains when that is on screen. */
	const STEPS: { title: string; text: string; keys?: string[]; target?: string }[] = [
		{
			title: 'Welcome to Kubri',
			text: 'A quick look at the four things that get you anywhere. It takes half a minute, and Help (?) brings it back.'
		},
		{
			title: 'Find anything with the palette',
			text: 'Press Ctrl+K or : to jump to a kind, a namespace, a context or an object, or to run an action on the row you chose.',
			keys: ['Ctrl+K', ':'],
			target: '[data-tour="palette"]'
		},
		{
			title: 'Right click for everything',
			text: 'A right click or the ⋯ button on a row lists every action for it: logs, shell, scale, restart, delete. The action bar above the list shows the common ones.',
			keys: ['right click'],
			target: 'main tbody tr.row'
		},
		{
			title: 'Keys, if you like them',
			text: 'Move with j and k, open with Enter, describe with d, logs with l. Press ? any time for all of them; the bar at the bottom shows the ones that work here.',
			keys: ['j', 'k', 'Enter', '?'],
			target: '[data-tour="keys"]'
		},
		{
			title: 'Read-only keeps you safe',
			text: 'This switch turns changes off. Guard rails in the context settings can force it for production clusters.',
			target: '[data-tour="read-only"]'
		}
	];

	let step = $state(0);
	const last = $derived(step === STEPS.length - 1);

	function finish() {
		app.tourOpen = false;
		app.markTourSeen();
	}

	function onkeydown(e: KeyboardEvent) {
		if (e.key === 'Escape') {
			e.preventDefault();
			e.stopPropagation();
			finish();
		} else if (e.key === 'Enter' || e.key === 'ArrowRight') {
			e.preventDefault();
			e.stopPropagation();
			if (last) finish();
			else step++;
		} else if (e.key === 'ArrowLeft') {
			e.preventDefault();
			e.stopPropagation();
			step = Math.max(0, step - 1);
		}
	}

	let box = $state<HTMLDivElement>();
	$effect(() => {
		box?.focus();
	});

	/** Where the step's element is on screen, or null to show the card in the middle. */
	let spot = $state<{ left: number; top: number; width: number; height: number } | null>(null);
	let width = $state(0);
	let height = $state(0);
	let viewWidth = $state(0);
	let viewHeight = $state(0);

	function locate() {
		const target = STEPS[step].target;
		const r = target ? document.querySelector(target)?.getBoundingClientRect() : undefined;
		spot = r && r.width > 0 && r.height > 0 ? { left: r.left, top: r.top, width: r.width, height: r.height } : null;
	}
	$effect(() => {
		void step;
		locate();
	});

	/** The card next to the element: below it when there is room, otherwise above, with an arrow at it. */
	const placed = $derived.by(() => {
		if (!spot || !width || !height) return null;
		const gap = 14;
		const below = spot.top + spot.height + gap + height <= viewHeight - 8;
		const left = Math.max(16, Math.min(spot.left + spot.width / 2 - width / 2, viewWidth - width - 16));
		const top = below ? spot.top + spot.height + gap : Math.max(8, spot.top - gap - height);
		const arrow = Math.max(20, Math.min(spot.left + spot.width / 2 - left, width - 20));
		return { left, top, arrow, below };
	});
</script>

<svelte:window onkeydowncapture={onkeydown} onresize={locate} bind:innerWidth={viewWidth} bind:innerHeight={viewHeight} />

<div class="backdrop" class:clear={spot} role="presentation"></div>
{#if spot}
	<div
		class="spot"
		aria-hidden="true"
		style:left="{spot.left - 4}px"
		style:top="{spot.top - 4}px"
		style:width="{spot.width + 8}px"
		style:height="{spot.height + 8}px"
	></div>
{/if}
<div
	class="tour"
	class:pointing={placed}
	role="dialog"
	aria-modal="true"
	aria-label="Quick tour"
	tabindex="-1"
	bind:this={box}
	bind:offsetWidth={width}
	bind:offsetHeight={height}
	style:left={placed ? `${placed.left}px` : null}
	style:top={placed ? `${placed.top}px` : null}
>
	{#if placed}
		<span class="arrow" class:up={placed.below} class:down={!placed.below} style:left="{placed.arrow}px" aria-hidden="true"></span>
	{/if}
	{#if step > 0}<p class="count">{step} of {STEPS.length - 1}</p>{/if}
	<h2>{STEPS[step].title}</h2>
	<p>{STEPS[step].text}</p>
	{#if STEPS[step].keys}
		<p class="keys">{#each STEPS[step].keys as key (key)}<kbd>{key}</kbd>{/each}</p>
	{/if}
	<div class="buttons">
		<button class="skip" onclick={finish}>Skip</button>
		<span class="spacer"></span>
		{#if step > 0}<button onclick={() => step--}>Back</button>{/if}
		<button class="primary" onclick={() => (last ? finish() : step++)}>{last ? 'Done' : 'Next'}</button>
	</div>
</div>

<style>
	.backdrop {
		position: fixed;
		inset: 0;
		z-index: 60;
		background: rgb(0 0 0 / 40%);
	}
	.tour {
		position: fixed;
		top: 50%;
		left: 50%;
		z-index: 61;
		width: min(440px, calc(100vw - 32px));
		padding: 22px 24px 18px;
		transform: translate(-50%, -50%);
		border: 1px solid var(--border);
		border-radius: 12px;
		background: var(--surface-1);
		box-shadow: var(--shadow);
	}
	.tour.pointing {
		transform: none;
	}
	.tour:focus {
		outline: none;
	}
	.backdrop.clear {
		background: none;
	}
	/* Lights up the element the step explains; its shadow darkens the rest. */
	.spot {
		position: fixed;
		z-index: 60;
		border-radius: 10px;
		outline: 2px solid var(--accent);
		box-shadow: 0 0 0 9999px rgb(0 0 0 / 40%);
		pointer-events: none;
	}
	.arrow {
		position: absolute;
		width: 12px;
		height: 12px;
		margin-left: -6px;
		border: 1px solid var(--border);
		background: var(--surface-1);
		transform: rotate(45deg);
	}
	.arrow.up {
		top: -7px;
		border-right: none;
		border-bottom: none;
	}
	.arrow.down {
		bottom: -7px;
		border-top: none;
		border-left: none;
	}
	.count {
		margin: 0 0 4px;
		font-size: 11.5px;
		color: var(--text-muted);
	}
	h2 {
		margin: 0 0 8px;
		font-size: 18px;
	}
	p {
		margin: 0 0 10px;
		font-size: 13.5px;
		line-height: 1.5;
	}
	.keys {
		display: flex;
		flex-wrap: wrap;
		gap: 6px;
	}
	kbd {
		padding: 1px 7px;
		border: 1px solid var(--border);
		border-radius: 4px;
		background: var(--surface-2);
		font-family: var(--mono);
		font-size: 12px;
	}
	.buttons {
		display: flex;
		align-items: center;
		gap: 8px;
		margin-top: 14px;
	}
	.spacer {
		flex: 1;
	}
	.buttons button {
		padding: 8px 14px;
		border-radius: 8px;
		border: 1px solid var(--border);
		background: var(--surface-1);
	}
	.buttons .primary {
		border-color: var(--accent);
		background: var(--accent);
		color: var(--on-accent);
	}
	.buttons .skip {
		border: none;
		background: none;
		color: var(--text-muted);
	}
</style>
