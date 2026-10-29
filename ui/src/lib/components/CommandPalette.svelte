<script lang="ts">
	import { onMount } from 'svelte';
	import { backend, type SearchHit } from '../api';
	import { suggest, type Suggestion } from '../commands';
	import type { RecentObject } from '../search';
	import { app } from '../state.svelte';

	let input = $state('');
	let active = $state(0);
	let field = $state<HTMLInputElement>();
	/** Objects whose name has what is typed, found as it is typed. */
	let hits = $state.raw<SearchHit[]>([]);
	let asked = 0;

	const term = $derived(input.trim().replace(/^:/, ''));
	/** One word that might be part of an object's name rather than a command. */
	const lookup = $derived(term.length >= 3 && !/\s/.test(term) && !term.includes(':') ? term : '');

	const where = (namespace: string | null | undefined, context: string | undefined) =>
		[namespace, context && context !== app.cluster?.context ? context : ''].filter(Boolean).join(' · ');
	const recentSuggestion = (o: RecentObject): Suggestion => ({
		label: o.name,
		detail: [o.kind, where(o.namespace, o.context), 'recent'].filter(Boolean).join(' · '),
		action: {
			type: 'object',
			target: { context: o.context, resource: { group: o.group, version: o.version, plural: o.plural }, namespace: o.namespace, name: o.name },
			kind: o.kind
		}
	});

	const objects = $derived.by((): Suggestion[] => {
		const recent = app.recent.filter((o) => o.name.toLowerCase().includes(lookup.toLowerCase())).slice(0, 4).map(recentSuggestion);
		const seen = new Set(recent.map((s) => `${s.detail}/${s.label}`));
		const found = hits
			.map((h): Suggestion => ({
				label: h.object.name,
				detail: [h.kind, where(h.object.namespace, h.object.context)].filter(Boolean).join(' · '),
				action: { type: 'object', target: h.object, kind: h.kind }
			}))
			.filter((s) => !seen.has(`${s.detail} · recent/${s.label}`));
		return [...recent, ...found];
	});

	$effect(() => {
		const text = lookup;
		const mine = ++asked;
		hits = [];
		if (!text || !app.cluster) return;
		const timer = setTimeout(async () => {
			try {
				const result = await backend.search({ text, kinds: [], allKinds: false, namespace: null, labels: '', fields: [], contexts: [], limit: 8 });
				if (mine === asked) hits = result.hits;
			} catch {
				// The palette still offers commands.
			}
		}, 250);
		return () => clearTimeout(timer);
	});

	const suggestions = $derived(
		input.trim()
			? [
					...suggest(input, app.resources, app.contexts, app.namespaces, app.paletteConfig),
					...(lookup ? objects : []),
					...(/^(search|find|grep)\b/i.test(term)
						? []
						: [{ label: `search ${term}`, detail: 'Search objects by name, label and field', action: { type: 'search', query: term } } as Suggestion])
				]
			: [
					...app.recent.slice(0, 5).map(recentSuggestion),
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

	// Closing gives the focus back to where it was, if that is still on screen.
	$effect(() => {
		const previous = document.activeElement as HTMLElement | null;
		return () => {
			if (previous?.isConnected) previous.focus();
		};
	});

	// `:ctx` offers contexts added to the kubeconfig since Kubri started.
	onMount(() => {
		app.reloadContexts(true);
	});

	function close() {
		app.paletteOpen = false;
	}

	function run(s: Suggestion | undefined) {
		if (!s) return;
		close();
		app.run(s.action);
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
<div class="palette" role="dialog" aria-modal="true" aria-label="Command palette">
	<div class="prompt">
		<span>:</span>
		<input
			bind:this={field}
			bind:value={input}
			{onkeydown}
			role="combobox"
			aria-label="Command"
			aria-expanded="true"
			aria-controls="palette-suggestions"
			aria-autocomplete="list"
			aria-activedescendant={suggestions.length ? `palette-option-${active}` : undefined}
			placeholder="pods, deploy kube-system, po app=web, ctx, ns default, skin…"
			spellcheck="false"
			autocomplete="off"
		/>
	</div>
	<ul id="palette-suggestions" role="listbox" aria-label="Suggestions">
		{#each suggestions as s, i (s.label + i)}
			<li role="presentation">
				<!-- Not mouseenter: a list that opens under a resting pointer keeps its first entry. -->
				<button
					id="palette-option-{i}"
					role="option"
					aria-selected={i === active}
					tabindex="-1"
					class:active={i === active}
					onmousemove={() => (active = i)}
					onclick={() => run(s)}
				>
					<span class="label">{s.label}</span>
					<span class="detail">{s.detail}</span>
				</button>
			</li>
		{:else}
			<li class="empty" role="status">No matching resource or command</li>
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
