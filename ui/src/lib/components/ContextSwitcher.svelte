<script lang="ts">
	import { app } from '../state.svelte';
	import Icon from './Icon.svelte';

	let query = $state('');
	let input = $state<HTMLInputElement>();

	const filtered = $derived(
		app.contexts.filter((c) => c.name.toLowerCase().includes(query.toLowerCase()))
	);

	function toggle() {
		app.contextMenuOpen = !app.contextMenuOpen;
		query = '';
		if (app.contextMenuOpen) {
			queueMicrotask(() => input?.focus());
			// Picks up contexts added to the kubeconfig since Kubri started.
			app.reloadContexts(true);
		}
	}

	function choose(name: string) {
		app.contextMenuOpen = false;
		if (name !== app.cluster?.context) app.switchContext(name);
	}

	function onkeydown(e: KeyboardEvent) {
		if (e.key === 'Escape') app.contextMenuOpen = false;
		if (e.key === 'Enter' && filtered[0]) choose(filtered[0].name);
	}

	function onwindowkeydown(e: KeyboardEvent) {
		// Ctrl/Cmd+Shift+C opens the switcher from anywhere.
		if ((e.ctrlKey || e.metaKey) && e.shiftKey && e.key.toLowerCase() === 'c') {
			e.preventDefault();
			toggle();
		}
	}
</script>

<svelte:window onkeydown={onwindowkeydown} />

<div class="switcher">
	<button
		class="current"
		onclick={toggle}
		title={app.cluster && !app.connecting
			? `${app.cluster.server} · ${app.cluster.version}\nSwitch context (Ctrl+Shift+C)`
			: 'Switch context (Ctrl+Shift+C)'}
		aria-haspopup="listbox"
		aria-expanded={app.contextMenuOpen}
	>
		<span class="dot" class:connecting={app.connecting} class:online={app.cluster && !app.connecting}></span>
		<span class="label">
			<span class="meta">{app.connecting ? 'Connecting…' : 'Context'}</span>
			<span class="name">{app.connecting ?? app.cluster?.context ?? 'No context'}</span>
		</span>
		<span class="chevron"><Icon name="chevron" size={16} /></span>
	</button>

	{#if app.contextMenuOpen}
		<div class="backdrop" onclick={() => (app.contextMenuOpen = false)} role="presentation"></div>
		<div class="menu" role="listbox">
			<div class="search">
				<input bind:this={input} bind:value={query} {onkeydown} aria-label="Search contexts" placeholder="Search contexts…" />
				<button class="reload" onclick={() => app.reloadContexts()} title="Reload kubeconfig">↻</button>
			</div>
			<ul>
				{#each filtered as ctx (ctx.name)}
					<li>
						<button
							class:active={ctx.name === app.cluster?.context}
							onclick={() => choose(ctx.name)}
							role="option"
							aria-selected={ctx.name === app.cluster?.context}
						>
							<span class="name">{ctx.name}</span>
							<span class="meta">
								{ctx.cluster ?? ''}{ctx.namespace ? ` · ns ${ctx.namespace}` : ''}
								{#if ctx.isDefault}<span class="badge">default</span>{/if}
							</span>
						</button>
					</li>
				{:else}
					<li class="empty">No matching contexts</li>
				{/each}
			</ul>
		</div>
	{/if}
</div>

<style>
	.switcher {
		position: relative;
	}
	.current {
		display: flex;
		align-items: center;
		gap: 10px;
		min-width: 150px;
		max-width: 260px;
		padding: 5px 8px;
		border: 1px solid transparent;
		border-radius: 8px;
		background: none;
		text-align: left;
	}
	.current:hover {
		background: var(--surface-2);
	}
	.current .meta {
		font-size: 10.5px;
	}
	.label {
		display: flex;
		flex-direction: column;
		min-width: 0;
		flex: 1;
	}
	.name {
		font-weight: 600;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.meta {
		font-size: 12px;
		color: var(--text-muted);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.chevron {
		display: flex;
		color: var(--text-muted);
	}
	.dot {
		width: 8px;
		height: 8px;
		border-radius: 50%;
		background: var(--text-muted);
		flex: none;
	}
	.dot.online {
		background: var(--ok);
		box-shadow: 0 0 0 3px color-mix(in srgb, var(--ok) 25%, transparent);
	}
	.dot.connecting {
		background: var(--warn);
		animation: pulse 1s infinite alternate;
	}
	@keyframes pulse {
		to {
			opacity: 0.3;
		}
	}
	.backdrop {
		position: fixed;
		inset: 0;
		z-index: 10;
	}
	.menu {
		position: absolute;
		top: calc(100% + 6px);
		left: 0;
		z-index: 11;
		width: min(420px, calc(100vw - 32px));
		max-height: 60vh;
		display: flex;
		flex-direction: column;
		background: var(--surface-1);
		border: 1px solid var(--border);
		border-radius: 10px;
		box-shadow: var(--shadow);
		overflow: hidden;
	}
	.search {
		display: flex;
		gap: 6px;
		padding: 8px;
		border-bottom: 1px solid var(--border);
	}
	.search input {
		flex: 1;
	}
	.reload {
		width: 32px;
		border-radius: 6px;
		background: var(--surface-2);
		border: 1px solid var(--border);
	}
	ul {
		list-style: none;
		margin: 0;
		padding: 4px;
		overflow-y: auto;
	}
	li button {
		display: flex;
		flex-direction: column;
		width: 100%;
		padding: 8px 10px;
		border-radius: 6px;
		text-align: left;
		background: none;
		border: none;
	}
	li button:hover {
		background: var(--surface-2);
	}
	li button.active {
		background: color-mix(in srgb, var(--accent) 15%, transparent);
	}
	.badge {
		margin-left: 6px;
		padding: 0 6px;
		border-radius: 4px;
		font-size: 11px;
		background: var(--surface-2);
		color: var(--text);
	}
	.empty {
		padding: 12px;
		color: var(--text-muted);
	}
</style>
