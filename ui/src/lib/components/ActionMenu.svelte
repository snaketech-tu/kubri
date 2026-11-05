<script lang="ts">
	import { tick } from 'svelte';
	import { actionFor, hintOf, keyOf, menuEntries, type Menu, type MenuAction } from '../menu';
	import { app } from '../state.svelte';

	let { menu }: { menu: Menu } = $props();

	let element = $state<HTMLDivElement>();
	let position = $state({ left: 0, top: 0 });
	let active = $state(0);

	/** The folds that are open, such as Copy. */
	let open = $state(new Set<string>());
	const groups = $derived(menuEntries(menu.actions, open));
	/** The lines in the order the menu shows them, for the arrow keys. */
	const flat = $derived(groups.flatMap(([, entries]) => entries));

	function toggle(id: string, show = !open.has(id)) {
		const next = new Set(open);
		if (show) next.add(id);
		else next.delete(id);
		open = next;
	}

	// A new menu starts at its first line, with its folds closed.
	$effect(() => {
		void menu;
		active = 0;
		open = new Set();
		tick().then(() => element?.querySelector<HTMLButtonElement>('button')?.focus());
	});

	// Opens at the point, moved back in where it would run off the window, also after a fold opens.
	$effect(() => {
		void menu;
		void open;
		tick().then(() => {
			if (!element) return;
			const { width, height } = element.getBoundingClientRect();
			const left = menu.align === 'right' ? menu.x - width : menu.x;
			position = {
				left: Math.max(8, Math.min(left, window.innerWidth - width - 8)),
				top: Math.max(8, Math.min(menu.y, window.innerHeight - height - 8))
			};
		});
	});

	function close() {
		app.menu = null;
	}

	function run(action: MenuAction) {
		close();
		action.run();
	}

	function focusItem(index: number) {
		active = (index + flat.length) % flat.length;
		element?.querySelectorAll<HTMLButtonElement>('button')[active]?.focus();
	}

	function choose(entry: (typeof flat)[number]) {
		if (entry.kind === 'fold') toggle(entry.id);
		else run(entry.action);
	}

	function onkeydown(e: KeyboardEvent) {
		const handled = (() => {
			switch (e.key) {
				case 'Escape':
				case 'Tab':
					close();
					return true;
				case 'ArrowDown':
					focusItem(active + 1);
					return true;
				case 'ArrowUp':
					focusItem(active - 1);
					return true;
				case 'Home':
					focusItem(0);
					return true;
				case 'End':
					focusItem(flat.length - 1);
					return true;
				case 'Enter':
				case ' ':
					if (flat[active]) choose(flat[active]);
					return true;
				case 'ArrowRight':
				case 'ArrowLeft': {
					const entry = flat[active];
					if (entry?.kind !== 'fold') return false;
					toggle(entry.id, e.key === 'ArrowRight');
					return true;
				}
			}
			// The keys work here too, also for actions in a closed fold, which is how the menu teaches them.
			const action = menu.hints === false ? undefined : actionFor(menu.actions, keyOf(e));
			if (action) run(action);
			return !!action;
		})();
		if (handled) {
			e.preventDefault();
			e.stopPropagation();
		}
	}
</script>

<svelte:window onresize={close} onblur={close} />

<div
	class="backdrop"
	role="presentation"
	onmousedown={close}
	oncontextmenu={(e) => {
		e.preventDefault();
		close();
	}}
></div>
<div
	class="menu"
	role="menu"
	tabindex="-1"
	bind:this={element}
	style:left="{position.left}px"
	style:top="{position.top}px"
	{onkeydown}
>
	{#each groups as [group, entries], g (g)}
		{#if g > 0}<div class="separator" role="separator"></div>{/if}
		{#if group}<div class="group" aria-hidden="true">{group}</div>{/if}
		{#each entries as entry (entry.kind === 'fold' ? `fold:${entry.id}` : `${entry.nested}:${entry.action.label}`)}
			{#if entry.kind === 'fold'}
				<button
					role="menuitem"
					class="fold"
					aria-haspopup="true"
					aria-expanded={open.has(entry.id)}
					onclick={() => toggle(entry.id)}
					onmouseenter={(e) => {
						active = flat.indexOf(entry);
						e.currentTarget.focus();
					}}
				>
					<span>{entry.label}</span>
					<span class="count">{entry.actions.length}</span>
					<span class="chevron" aria-hidden="true">{open.has(entry.id) ? '▾' : '▸'}</span>
				</button>
			{:else}
				{@const action = entry.action}
				{@const hint = menu.hints === false ? null : hintOf(menu.actions, action)}
				<button
					role="menuitem"
					class:nested={entry.nested}
					class:danger={action.danger}
					class:refused={action.disabled}
					aria-disabled={action.disabled ? 'true' : undefined}
					title={action.disabled ? `${action.disabled}. Choose it for details.` : undefined}
					onclick={() => run(action)}
					onmouseenter={(e) => {
						active = flat.indexOf(entry);
						e.currentTarget.focus();
					}}
				>
					<span>{action.label}</span>
					{#if action.disabled}<span class="lock" aria-label="not allowed">⊘</span>{/if}
					{#if hint}<kbd>{hint}</kbd>{/if}
				</button>
			{/if}
		{/each}
	{/each}
</div>

<style>
	.backdrop {
		position: fixed;
		inset: 0;
		z-index: 40;
	}
	.menu {
		position: fixed;
		z-index: 41;
		min-width: 220px;
		max-width: 320px;
		max-height: calc(100vh - 16px);
		overflow-y: auto;
		padding: 4px;
		border: 1px solid var(--border);
		border-radius: 10px;
		background: var(--surface-1);
		box-shadow: var(--shadow);
		outline: none;
	}
	.group {
		padding: 6px 10px 3px;
		font-size: 10.5px;
		font-weight: 600;
		letter-spacing: 0.06em;
		text-transform: uppercase;
		color: var(--text-muted);
	}
	.separator {
		height: 1px;
		margin: 4px 6px;
		background: var(--border);
	}
	button {
		display: flex;
		align-items: center;
		gap: 16px;
		width: 100%;
		padding: 5px 10px;
		border: none;
		border-radius: 6px;
		background: none;
		text-align: left;
		white-space: nowrap;
	}
	button:focus {
		outline: none;
		background: color-mix(in srgb, var(--accent) 16%, transparent);
	}
	button.danger {
		color: var(--bad);
	}
	button.danger:focus {
		background: color-mix(in srgb, var(--bad) 14%, transparent);
	}
	button span {
		overflow: hidden;
		text-overflow: ellipsis;
	}
	kbd {
		margin-left: auto;
		padding: 0 5px;
		border: 1px solid var(--border);
		border-radius: 4px;
		background: var(--surface-2);
		font-family: var(--mono);
		font-size: 11px;
		color: var(--text-muted);
	}
	button.refused {
		color: var(--text-muted);
	}
	button.nested {
		padding-left: 22px;
	}
	.fold .count {
		margin-left: auto;
		font-size: 11px;
		color: var(--text-muted);
	}
	.fold .chevron {
		width: 10px;
		font-size: 11px;
		color: var(--text-muted);
	}
	.lock {
		margin-left: auto;
		font-size: 12px;
	}
	.lock + kbd {
		margin-left: 6px;
	}
</style>
