<script lang="ts">
	import { tick } from 'svelte';
	import { actionFor, grouped, hintOf, keyOf, type Menu, type MenuAction } from '../menu';
	import { app } from '../state.svelte';

	let { menu }: { menu: Menu } = $props();

	let element = $state<HTMLDivElement>();
	let position = $state({ left: 0, top: 0 });
	let active = $state(0);

	const groups = $derived(grouped(menu.actions));
	/** In the order the menu shows them, for the arrow keys. */
	const flat = $derived(groups.flatMap(([, actions]) => actions));

	// Opens at the point, moved back in where it would run off the window.
	$effect(() => {
		void menu;
		active = 0;
		tick().then(() => {
			if (!element) return;
			const { width, height } = element.getBoundingClientRect();
			const left = menu.align === 'right' ? menu.x - width : menu.x;
			position = {
				left: Math.max(8, Math.min(left, window.innerWidth - width - 8)),
				top: Math.max(8, Math.min(menu.y, window.innerHeight - height - 8))
			};
			element.querySelector<HTMLButtonElement>('button')?.focus();
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
					if (flat[active]) run(flat[active]);
					return true;
			}
			// The keys work here too, which is how the menu teaches them.
			const action = menu.hints === false ? undefined : actionFor(flat, keyOf(e));
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
	{#each groups as [group, actions], g (g)}
		{#if g > 0}<div class="separator" role="separator"></div>{/if}
		<div class="group" aria-hidden="true">{group}</div>
		{#each actions as action, i (i)}
			{@const hint = menu.hints === false ? null : hintOf(flat, action)}
			<button
				role="menuitem"
				class:danger={action.danger}
				class:refused={action.disabled}
				aria-disabled={action.disabled ? 'true' : undefined}
				title={action.disabled ? `${action.disabled}. Choose it for details.` : undefined}
				onclick={() => run(action)}
				onmouseenter={(e) => {
					active = flat.indexOf(action);
					e.currentTarget.focus();
				}}
			>
				<span>{action.label}</span>
				{#if action.disabled}<span class="lock" aria-label="not allowed">⊘</span>{/if}
				{#if hint}<kbd>{hint}</kbd>{/if}
			</button>
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
	.lock {
		margin-left: auto;
		font-size: 12px;
	}
	.lock + kbd {
		margin-left: 6px;
	}
</style>
