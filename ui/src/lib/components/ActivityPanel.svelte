<script lang="ts">
	import { age } from '../format';
	import { session } from '../session.svelte';
	import { app } from '../state.svelte';
	import { describeChange, teamActivity, type TeamActivity } from '../team';

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

	/** On a Kubri shared by a team, the changes in its audit log, besides this window's. */
	let tab = $state<'window' | 'team'>('window');
	let team = $state<TeamActivity | null>(null);
	let teamError = $state<string | null>(null);

	$effect(() => {
		if (tab !== 'team') return;
		let stopped = false;
		const load = () =>
			teamActivity()
				.then((activity) => {
					if (stopped) return;
					team = activity;
					teamError = null;
				})
				.catch((err) => {
					if (!stopped) teamError = err instanceof Error ? err.message : String(err);
				});
		load();
		const timer = setInterval(load, 10_000);
		return () => {
			stopped = true;
			clearInterval(timer);
		};
	});

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
		<h2>Activity <span class="n">{tab === 'team' ? (team?.entries.length ?? 0) : app.activity.length}</span></h2>
		<button onclick={close} aria-label="Close">×</button>
	</header>
	{#if session.person}
		<div class="tabs" role="tablist" aria-label="Whose changes">
			<button role="tab" aria-selected={tab === 'window'} onclick={() => (tab = 'window')}>This window</button>
			<button role="tab" aria-selected={tab === 'team'} onclick={() => (tab = 'team')}>Team</button>
		</div>
	{/if}
	{#if tab === 'team'}
		<p class="muted">
			{team?.everyone
				? 'Changes everyone made through this Kubri, newest first.'
				: 'Changes you made through this Kubri, from any window, newest first. People in the activity groups see everyone’s.'}
		</p>
		{#if teamError}
			<p class="empty failed-text" role="alert">{teamError}</p>
		{:else if team?.entries.length}
			<ul>
				{#each team.entries as entry, i (i)}
					<li class:failed={!entry.ok} title={entry.error}>
						<span class="when">{age(entry.time, now) || '0s'}</span>
						<span class="what">
							<span class="text">{describeChange(entry)}</span>
							<span class="where">{[entry.person, entry.context].filter(Boolean).join(' · ')}</span>
							{#if entry.error}<span class="where failed-text">{entry.error}</span>{/if}
						</span>
					</li>
				{/each}
			</ul>
		{:else if team}
			<p class="empty">No changes yet since the server started.</p>
		{/if}
	{:else}
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
	li.failed .text,
	.failed-text {
		color: var(--bad);
	}
	.tabs {
		display: flex;
		gap: 4px;
		margin: 10px 0 2px;
		padding: 3px;
		border-radius: 8px;
		background: var(--surface-2);
	}
	.tabs button {
		flex: 1;
		padding: 5px 10px;
		border: none;
		border-radius: 6px;
		background: none;
		color: var(--text-muted);
		font-size: 12.5px;
		font-weight: 600;
	}
	.tabs button[aria-selected='true'] {
		background: var(--surface-1);
		color: var(--text);
		box-shadow: 0 1px 2px rgb(16 24 40 / 10%);
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
