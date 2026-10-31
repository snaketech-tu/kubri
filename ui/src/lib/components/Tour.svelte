<script lang="ts">
	import { app } from '../state.svelte';

	const STEPS: { title: string; text: string; keys?: string[] }[] = [
		{
			title: 'Welcome to Kubri',
			text: 'A quick look at the four things that get you anywhere. It takes half a minute, and Help (?) brings it back.'
		},
		{
			title: 'Find anything with the palette',
			text: 'Press Ctrl+K or : to jump to a kind, a namespace, a context or an object, or to run an action on the row you chose.',
			keys: ['Ctrl+K', ':']
		},
		{
			title: 'Right click for everything',
			text: 'A right click or the ⋯ button on a row lists every action for it: logs, shell, scale, restart, delete. The action bar above the list shows the common ones.',
			keys: ['right click']
		},
		{
			title: 'Keys, if you like them',
			text: 'Move with j and k, open with Enter, describe with d, logs with l. Press ? any time for all of them; the bar at the bottom shows the ones that work here.',
			keys: ['j', 'k', 'Enter', '?']
		},
		{
			title: 'Read-only keeps you safe',
			text: 'The switch at the top right turns changes off. Guard rails in the context settings can force it for production clusters.'
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
</script>

<svelte:window onkeydowncapture={onkeydown} />

<div class="backdrop" role="presentation"></div>
<div class="tour" role="dialog" aria-modal="true" aria-label="Quick tour" tabindex="-1" bind:this={box}>
	<p class="count">{step + 1} of {STEPS.length}</p>
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
	.tour:focus {
		outline: none;
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
