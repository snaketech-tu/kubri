<script lang="ts">
	import { dock, type DockSession } from '../dock.svelte';
	import DockPane from './DockPane.svelte';
	import Icon from './Icon.svelte';

	const MIN = 160;
	let section = $state<HTMLElement>();

	const right = $derived(dock.side === 'right' && !dock.minimized);
	const shown = (id: number) => !dock.minimized && (id === dock.active || id === dock.beside);

	function label(s: DockSession) {
		return s.container ? `${s.target.name} · ${s.container}` : s.target.name;
	}

	/** Keeps the list at least 240px and the workspace at least 160px. */
	function resize(size: number) {
		const room = section?.parentElement?.[right ? 'clientWidth' : 'clientHeight'] ?? window.innerHeight;
		dock.size = Math.max(MIN, Math.min(room - 240, size));
	}

	function onpointerdown(e: PointerEvent) {
		if (e.button !== 0 || !section) return;
		e.preventDefault();
		const handle = e.currentTarget as HTMLElement;
		handle.setPointerCapture(e.pointerId);
		const box = section.getBoundingClientRect();
		const move = (ev: PointerEvent) => resize(right ? box.right - ev.clientX : box.bottom - ev.clientY);
		const up = () => {
			handle.removeEventListener('pointermove', move);
			handle.removeEventListener('pointerup', up);
			handle.removeEventListener('pointercancel', up);
		};
		handle.addEventListener('pointermove', move);
		handle.addEventListener('pointerup', up);
		handle.addEventListener('pointercancel', up);
	}

	function onresizekey(e: KeyboardEvent) {
		const step = e.shiftKey ? 80 : 20;
		const current = section ? (right ? section.clientWidth : section.clientHeight) : 0;
		const grow = right ? 'ArrowLeft' : 'ArrowUp';
		const shrink = right ? 'ArrowRight' : 'ArrowDown';
		if (e.key === grow) resize(current + step);
		else if (e.key === shrink) resize(current - step);
		else if (e.key === 'Home') dock.size = 0;
		else return;
		e.preventDefault();
		e.stopPropagation();
	}

	/** Middle click closes a tab, as in browsers. */
	function onauxclick(e: MouseEvent, id: number) {
		if (e.button !== 1) return;
		e.preventDefault();
		dock.close(id);
	}

	/** The pane with the keyboard gets the log keys, and the next tab picked when split. */
	function onfocusin(id: number) {
		dock.focused = id;
		dock.last = id;
	}
	/** Focus dropped to the page, such as by Esc in the search, stays with the pane. */
	function onfocusout(e: FocusEvent, id: number) {
		const to = e.relatedTarget as Node | null;
		if (to && !(e.currentTarget as HTMLElement).contains(to) && dock.focused === id) dock.focused = null;
	}
</script>

<!-- A click elsewhere, such as on a row of the list, gives the keys back to the page. -->
<svelte:window onpointerdown={(e) => !section?.contains(e.target as Node) && (dock.focused = null)} />

<section
	class="dock"
	class:right
	class:minimized={dock.minimized}
	aria-label="Workspace"
	bind:this={section}
	style:--dock-size={dock.size ? `${dock.size}px` : null}
>
	{#if !dock.minimized}
		<!-- A focusable separator is a widget: it takes the arrow keys. -->
		<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
		<div
			class="resize"
			role="separator"
			aria-orientation={right ? 'vertical' : 'horizontal'}
			aria-label="Resize the workspace"
			tabindex="0"
			title="Drag to resize, double-click for the default size"
			{onpointerdown}
			ondblclick={() => (dock.size = 0)}
			onkeydown={onresizekey}
		></div>
	{/if}
	<div class="bar">
		<div class="tabs" role="tablist" aria-label="Logs and shells">
			{#each dock.sessions as s (s.id)}
				{@const on = shown(s.id)}
				<div class="tab" class:on class:focused={on && dock.beside !== null && dock.last === s.id}>
					<button
						role="tab"
						aria-selected={on}
						title="{s.kind === 'logs' ? 'Logs' : 'Shell'} of {s.resource.kind} {s.target.namespace ? `${s.target.namespace}/` : ''}{label(
							s
						)}{s.previous ? ' (previous instance)' : ''}"
						onclick={() => dock.show(s.id)}
						onauxclick={(e) => onauxclick(e, s.id)}
					>
						<Icon name={s.kind === 'logs' ? 'logs' : 'terminal'} size={14} />
						<span>{label(s)}</span>
						{#if s.previous}<em>previous</em>{/if}
					</button>
					<button class="close" onclick={() => dock.close(s.id)} title="Close" aria-label="Close {label(s)}">
						<Icon name="close" size={13} />
					</button>
				</div>
			{/each}
		</div>
		<div class="window">
			{#if !dock.minimized}
				<button
					class="icon"
					class:on={dock.beside !== null}
					aria-pressed={dock.beside !== null}
					disabled={dock.sessions.length < 2}
					onclick={() => dock.toggleSplit()}
					title={dock.beside !== null ? 'Show one at a time' : 'Show two side by side'}
					aria-label="Side by side"><Icon name="split" size={16} /></button
				>
				<button
					class="icon"
					onclick={() => (dock.side = dock.side === 'bottom' ? 'right' : 'bottom')}
					title={dock.side === 'bottom' ? 'Dock at the right' : 'Dock at the bottom'}
					aria-label={dock.side === 'bottom' ? 'Dock at the right' : 'Dock at the bottom'}
					><Icon name={dock.side === 'bottom' ? 'dockRight' : 'dockBottom'} size={16} /></button
				>
			{/if}
			<button
				class="icon"
				onclick={() => (dock.minimized = !dock.minimized)}
				title={dock.minimized ? 'Show the workspace' : 'Minimize to the task bar; logs and shells keep running'}
				aria-label={dock.minimized ? 'Show the workspace' : 'Minimize'}
				aria-expanded={!dock.minimized}><Icon name={dock.minimized ? 'up' : 'chevron'} size={16} /></button
			>
		</div>
	</div>
	<div class="panes" class:split={dock.beside !== null} hidden={dock.minimized}>
		{#each dock.sessions as s (s.id)}
			<div
				class="pane"
				class:hidden={!shown(s.id)}
				class:second={s.id === dock.beside}
				onfocusin={() => onfocusin(s.id)}
				onfocusout={(e) => onfocusout(e, s.id)}
				role="tabpanel"
				tabindex="-1"
				aria-label={label(s)}
			>
				<DockPane session={s} active={shown(s.id) && dock.focused === s.id} />
			</div>
		{/each}
	</div>
</section>

<style>
	.dock {
		position: relative;
		display: flex;
		flex-direction: column;
		flex: none;
		height: var(--dock-size, clamp(240px, 44vh, 520px));
		min-height: 0;
		border-top: 1px solid var(--border);
		background: var(--surface-1);
	}
	.dock.right {
		width: var(--dock-size, clamp(360px, 40vw, 640px));
		height: auto;
		border-top: none;
		border-left: 1px solid var(--border);
	}
	.dock.minimized {
		height: auto;
	}
	.resize {
		position: absolute;
		z-index: 3;
		top: -4px;
		left: 0;
		right: 0;
		height: 8px;
		cursor: row-resize;
		touch-action: none;
	}
	.right .resize {
		top: 0;
		bottom: 0;
		left: -4px;
		right: auto;
		width: 8px;
		height: auto;
		cursor: col-resize;
	}
	.resize:hover,
	.resize:focus-visible {
		outline: none;
		background: linear-gradient(to bottom, transparent 3px, var(--accent) 3px, var(--accent) 5px, transparent 5px);
	}
	.right .resize:hover,
	.right .resize:focus-visible {
		background: linear-gradient(to right, transparent 3px, var(--accent) 3px, var(--accent) 5px, transparent 5px);
	}
	.bar {
		display: flex;
		align-items: stretch;
		gap: 6px;
		min-height: 34px;
		padding: 0 6px 0 8px;
		border-bottom: 1px solid var(--border-subtle);
		background: var(--surface-2, var(--surface-1));
	}
	.minimized .bar {
		border-bottom: none;
	}
	.tabs {
		display: flex;
		flex: 1;
		min-width: 0;
		overflow-x: auto;
		scrollbar-width: thin;
	}
	.tab {
		display: flex;
		align-items: center;
		flex: none;
		max-width: 260px;
		border-bottom: 2px solid transparent;
		color: var(--text-muted);
	}
	.tab.on {
		border-bottom-color: var(--accent);
		color: var(--text);
	}
	.tab.focused {
		background: color-mix(in srgb, var(--accent) 8%, transparent);
	}
	.tab button {
		border: none;
		background: none;
		color: inherit;
	}
	.tab [role='tab'] {
		display: flex;
		align-items: center;
		gap: 6px;
		min-width: 0;
		padding: 6px 4px 6px 10px;
		font-family: var(--mono);
		font-size: 12px;
	}
	.tab [role='tab'] span {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.tab em {
		font-style: normal;
		font-family: var(--sans, inherit);
		font-size: 11px;
		color: var(--warn);
	}
	.tab .close {
		display: grid;
		place-items: center;
		width: 22px;
		height: 22px;
		margin-right: 4px;
		border-radius: 5px;
		opacity: 0.6;
	}
	.tab .close:hover {
		opacity: 1;
		background: var(--surface-3, color-mix(in srgb, var(--text) 8%, transparent));
	}
	.window {
		display: flex;
		align-items: center;
		gap: 2px;
	}
	.icon {
		display: grid;
		place-items: center;
		width: 28px;
		height: 28px;
		border: none;
		border-radius: 6px;
		background: none;
		color: var(--text-muted);
	}
	.icon:hover:not(:disabled),
	.icon.on {
		background: color-mix(in srgb, var(--text) 8%, transparent);
		color: var(--text);
	}
	.icon:disabled {
		opacity: 0.4;
	}
	.panes {
		display: flex;
		flex: 1;
		min-height: 0;
	}
	.panes[hidden] {
		display: none;
	}
	.pane {
		display: flex;
		flex-direction: column;
		flex: 1;
		min-width: 0;
		min-height: 0;
		outline: none;
	}
	.pane > :global(*) {
		flex: 1;
	}
	.pane.second {
		order: 1;
		border-left: 1px solid var(--border);
	}
	/* Beside the list there is more height than width: one above the other. */
	.right .panes {
		flex-direction: column;
	}
	.right .pane.second {
		border-left: none;
		border-top: 1px solid var(--border);
	}
	.pane.hidden {
		display: none;
	}
	@media (max-width: 720px) {
		.dock.right {
			width: auto;
		}
	}
</style>
