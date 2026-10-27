<script lang="ts">
	import { suggest, type Suggestion } from '../commands';
	import { app } from '../state.svelte';

	let input = $state('');
	let active = $state(0);
	let field = $state<HTMLInputElement>();

	const suggestions = $derived(
		input.trim()
			? suggest(input, app.resources, app.contexts, app.namespaces)
			: [
					...suggest('ctx', app.resources, app.contexts, app.namespaces).slice(0, 3),
					...suggest('po', app.resources, app.contexts, app.namespaces).slice(0, 1),
					...suggest('deploy', app.resources, app.contexts, app.namespaces).slice(0, 1),
					...suggest('svc', app.resources, app.contexts, app.namespaces).slice(0, 1)
				]
	);

	$effect(() => {
		void input;
		active = 0;
	});

	$effect(() => {
		field?.focus();
	});

	function close() {
		app.paletteOpen = false;
	}

	function run(s: Suggestion | undefined) {
		if (!s) return;
		close();
		const action = s.action;
		switch (action.type) {
			case 'resource':
				app.open(action.resource, action.namespace);
				break;
			case 'context':
				if (action.name !== app.cluster?.context) app.switchContext(action.name);
				break;
			case 'namespace':
				app.namespace = action.name;
				break;
			case 'portforwards':
				app.showPortForwards();
				break;
			case 'help':
				app.helpOpen = true;
				break;
		}
	}

	function onkeydown(e: KeyboardEvent) {
		switch (e.key) {
			case 'Escape':
				close();
				break;
			case 'ArrowDown':
				active = Math.min(suggestions.length - 1, active + 1);
				break;
			case 'ArrowUp':
				active = Math.max(0, active - 1);
				break;
			case 'Tab':
				// Complete the highlighted suggestion, like k9s.
				if (suggestions[active]) input = suggestions[active].label + ' ';
				break;
			case 'Enter':
				run(suggestions[active]);
				break;
			default:
				return;
		}
		e.preventDefault();
	}
</script>

<div class="backdrop" onclick={close} role="presentation"></div>
<div class="palette" role="dialog" aria-label="Command palette">
	<div class="prompt">
		<span>:</span>
		<input
			bind:this={field}
			bind:value={input}
			{onkeydown}
			placeholder="pods, deploy kube-system, ctx, ns default, crd…"
			spellcheck="false"
			autocomplete="off"
		/>
	</div>
	<ul>
		{#each suggestions as s, i (s.label + i)}
			<li>
				<button class:active={i === active} onmouseenter={() => (active = i)} onclick={() => run(s)}>
					<span class="label">{s.label}</span>
					<span class="detail">{s.detail}</span>
				</button>
			</li>
		{:else}
			<li class="empty">No matching resource or command</li>
		{/each}
	</ul>
	<footer><kbd>↑↓</kbd> select <kbd>Tab</kbd> complete <kbd>Enter</kbd> run <kbd>Esc</kbd> close</footer>
</div>

<style>
	.backdrop {
		position: fixed;
		inset: 0;
		z-index: 40;
		background: rgb(0 0 0 / 25%);
	}
	.palette {
		position: fixed;
		top: 12vh;
		left: 50%;
		z-index: 41;
		width: min(640px, calc(100vw - 32px));
		transform: translateX(-50%);
		display: flex;
		flex-direction: column;
		max-height: 70vh;
		border: 1px solid var(--border);
		border-radius: 12px;
		background: var(--surface-1);
		box-shadow: var(--shadow);
		overflow: hidden;
	}
	.prompt {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 12px 16px;
		border-bottom: 1px solid var(--border);
		font-family: var(--mono);
		font-size: 16px;
	}
	.prompt span {
		color: var(--accent-strong);
		font-weight: 700;
	}
	.prompt input {
		flex: 1;
		padding: 0;
		border: none;
		background: none;
		box-shadow: none;
		font-family: var(--mono);
		font-size: 16px;
	}
	ul {
		margin: 0;
		padding: 6px;
		list-style: none;
		overflow-y: auto;
	}
	li button {
		display: flex;
		justify-content: space-between;
		gap: 16px;
		width: 100%;
		padding: 8px 10px;
		border: none;
		border-radius: 6px;
		background: none;
		text-align: left;
	}
	li button.active {
		background: color-mix(in srgb, var(--accent) 16%, transparent);
	}
	.label {
		font-family: var(--mono);
		font-size: 13px;
	}
	.detail {
		color: var(--text-muted);
		font-size: 12px;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.empty {
		padding: 12px;
		color: var(--text-muted);
	}
	footer {
		padding: 8px 16px;
		border-top: 1px solid var(--border);
		font-size: 11px;
		color: var(--text-muted);
	}
	kbd {
		margin: 0 2px 0 8px;
		padding: 0 4px;
		border: 1px solid var(--border);
		border-radius: 4px;
		font-family: var(--mono);
	}
	kbd:first-child {
		margin-left: 0;
	}
</style>
