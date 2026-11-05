<script lang="ts">
	import { age } from '../format';
	import { app } from '../state.svelte';

	let box = $state<HTMLElement>();
	let now = $state(Date.now());

	$effect(() => {
		const timer = setInterval(() => (now = Date.now()), 1000);
		return () => clearInterval(timer);
	});

	// The panel takes the focus while open and gives it back when it closes.
	$effect(() => {
		const previous = document.activeElement as HTMLElement | null;
		box?.focus();
		return () => {
			if (previous?.isConnected) previous.focus();
		};
	});

	function close() {
		app.activityOpen = false;
	}

	// The page beside the panel keeps its keys; while the focus is in the panel, its keys stay here.
	function onkeydown(e: KeyboardEvent) {
		if (e.key === 'Escape') {
			e.preventDefault();
			close();
		}
		if (e.key !== 'Tab') e.stopPropagation();
	}
</script>

<!-- A side panel, not a dialog: the list stays in view and usable next to it. -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<aside class="panel" aria-label="Activity in this session" tabindex="-1" bind:this={box} {onkeydown}>
	<header>
		<h2>Activity <span class="n">{app.activity.length}</span></h2>
		<button onclick={close} aria-label="Close">×</button>
	</header>
	<p class="muted">Every change made from Kubri since it started. Undo is offered where Kubernetes allows it.</p>
	{#if app.activity.length}
		<ul>
			{#each app.activity as entry (entry.id)}
				<li class:failed={!entry.ok} class:undone={entry.undone}>
					<span class="when">{age(new Date(entry.at).toISOString(), now) || '0s'}</span>
					<span class="what">
						<span class="text">{entry.text}</span>
						<span class="where">{entry.context}</span>
					</span>
					{#if entry.undone}
						<span class="muted">Undone</span>
					{:else if entry.undo}
						<button class="undo" onclick={() => app.undoActivity(entry.id)}>Undo</button>
					{/if}
				</li>
			{/each}
		</ul>
	{:else}
		<p class="empty">Nothing yet. Changes you make, such as scaling or deleting, are listed here.</p>
	{/if}
</aside>

<style>
	.panel {
		position: absolute;
		top: 0;
		right: 0;
		bottom: 0;
		z-index: 30;
		display: flex;
		flex-direction: column;
		width: min(400px, 100%);
		padding: 16px 20px;
		border-left: 1px solid var(--border);
		background: var(--surface-1);
		box-shadow: -8px 0 24px rgb(16 24 40 / 10%);
	}
	.panel:focus {
		outline: none;
	}
	header {
		display: flex;
		align-items: center;
		justify-content: space-between;
	}
	h2 {
		margin: 0;
		font-size: 17px;
	}
	.n {
		padding: 0 6px;
		border-radius: 4px;
		background: var(--surface-2);
		font-size: 11px;
		color: var(--text-muted);
	}
	header button {
		border: none;
		background: none;
		font-size: 20px;
		color: var(--text-muted);
	}
	.muted,
	.empty {
		margin: 6px 0 10px;
		font-size: 12.5px;
		color: var(--text-muted);
	}
	ul {
		margin: 0;
		padding: 0;
		overflow: auto;
		list-style: none;
	}
	li {
		display: grid;
		grid-template-columns: 44px minmax(0, 1fr) auto;
		align-items: center;
		gap: 10px;
		padding: 9px 0;
		border-top: 1px solid var(--border-subtle);
		font-size: 13px;
	}
	.when {
		font-family: var(--mono);
		font-size: 11.5px;
		color: var(--text-muted);
	}
	.what {
		display: grid;
		min-width: 0;
	}
	.text {
		overflow-wrap: anywhere;
	}
	.where {
		font-size: 11.5px;
		color: var(--text-muted);
	}
	li.failed .text {
		color: var(--bad);
	}
	li.undone .text {
		text-decoration: line-through;
		color: var(--text-muted);
	}
	.undo {
		padding: 4px 12px;
		border: 1px solid var(--border);
		border-radius: 6px;
		background: var(--surface-1);
		font-size: 12px;
	}
</style>
