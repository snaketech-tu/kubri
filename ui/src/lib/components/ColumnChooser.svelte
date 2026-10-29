<script lang="ts">
	import type { ColumnSettings } from '../api';
	import { NO_COLUMNS, moved, toggled, type Arrangeable } from '../columns';
	import Icon from './Icon.svelte';

	let {
		kind,
		columns,
		settings,
		wide,
		onchange,
		onclose
	}: {
		kind: string;
		/** Every column the list can show, in the order it shows them. */
		columns: (Arrangeable & { custom?: boolean })[];
		settings: ColumnSettings;
		wide: boolean;
		onchange: (settings: ColumnSettings) => void;
		onclose: () => void;
	} = $props();

	let name = $state('');
	let path = $state('');
	let box = $state<HTMLDivElement>();

	const labels = $derived(columns.map((c) => c.label));
	/** Shown without the wide view; a column only the wide view shows says so. */
	const checked = (c: Arrangeable) =>
		!settings.hidden.includes(c.label) && (!c.wideOnly || settings.shown.includes(c.label));

	function add(e: SubmitEvent) {
		e.preventDefault();
		const p = path.trim();
		if (!p) return;
		const label = name.trim() || p;
		onchange({
			...settings,
			custom: [...settings.custom.filter((c) => c.path !== p), { name: label, path: p }]
		});
		name = '';
		path = '';
	}

	function remove(label: string) {
		onchange({
			...settings,
			custom: settings.custom.filter((c) => c.name !== label),
			hidden: settings.hidden.filter((l) => l !== label),
			order: settings.order.filter((l) => l !== label)
		});
	}

	function onkeydown(e: KeyboardEvent) {
		if (e.key === 'Escape') {
			e.preventDefault();
			e.stopPropagation();
			onclose();
		}
	}

	function onpointerdown(e: PointerEvent) {
		const t = e.target as Node;
		if (box && !box.contains(t) && !(t as HTMLElement).closest?.('.chooser')) onclose();
	}
</script>

<svelte:window {onpointerdown} />

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div class="popover" role="dialog" aria-label="Columns for {kind}" tabindex="-1" bind:this={box} {onkeydown}>
	<div class="head">
		<strong>Columns for {kind}</strong>
		<button class="link" onclick={() => onchange(NO_COLUMNS)} disabled={JSON.stringify(settings) === JSON.stringify(NO_COLUMNS)}>
			Reset
		</button>
	</div>
	<ul>
		{#each columns as c, i (c.label)}
			<li>
				<label>
					<input type="checkbox" checked={checked(c)} onchange={(e) => onchange(toggled(settings, c, e.currentTarget.checked))} />
					<span>{c.label}</span>
					{#if c.custom}<span class="tag">JSONPath</span>{/if}
					{#if c.wideOnly && !checked(c)}<span class="tag" title="Shows in the wide view{wide ? ', which is on' : ''}">wide</span>{/if}
				</label>
				<span class="moves">
					<button onclick={() => onchange(moved(settings, labels, c.label, -1))} disabled={i === 0} aria-label="Move {c.label} up" title="Move up">
						<Icon name="up" size={13} />
					</button>
					<button
						onclick={() => onchange(moved(settings, labels, c.label, 1))}
						disabled={i === columns.length - 1}
						aria-label="Move {c.label} down"
						title="Move down"
					>
						<Icon name="chevron" size={13} />
					</button>
					{#if c.custom}
						<button onclick={() => remove(c.label)} aria-label="Remove {c.label}" title="Remove"><Icon name="close" size={13} /></button>
					{/if}
				</span>
			</li>
		{/each}
	</ul>
	<form onsubmit={add}>
		<strong>Add a column</strong>
		<input bind:value={name} placeholder="Name, such as Node" aria-label="Column name" spellcheck="false" />
		<input bind:value={path} placeholder="JSONPath, such as .spec.nodeName" aria-label="JSONPath" spellcheck="false" required />
		<button type="submit" disabled={!path.trim()}>Add</button>
		<p class="hint">Like <code>kubectl get -o custom-columns</code>: <code>.spec.containers[*].image</code>, <code>.metadata.labels.app</code>.</p>
	</form>
</div>

<style>
	.popover {
		position: absolute;
		top: calc(100% + 6px);
		right: 0;
		z-index: 30;
		display: flex;
		flex-direction: column;
		gap: 8px;
		width: 320px;
		max-height: 70vh;
		padding: 12px;
		border: 1px solid var(--border);
		border-radius: 10px;
		background: var(--surface-1);
		box-shadow: var(--shadow);
		overflow: auto;
		font-size: 12.5px;
	}
	.head {
		display: flex;
		justify-content: space-between;
		align-items: center;
	}
	ul {
		margin: 0;
		padding: 0;
		list-style: none;
	}
	li {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 8px;
		padding: 2px 0;
	}
	label {
		display: flex;
		align-items: center;
		gap: 6px;
		min-width: 0;
	}
	label span:not(.tag) {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.tag {
		padding: 0 5px;
		border-radius: 4px;
		background: var(--surface-2);
		color: var(--text-muted);
		font-size: 10.5px;
	}
	.moves {
		display: flex;
		gap: 2px;
		flex: none;
	}
	.moves button {
		display: grid;
		place-items: center;
		width: 24px;
		height: 22px;
		padding: 0;
		border: 1px solid var(--border-subtle);
		border-radius: 5px;
		background: none;
		color: var(--text-muted);
	}
	.moves button:disabled {
		opacity: 0.35;
	}
	form {
		display: grid;
		gap: 6px;
		padding-top: 8px;
		border-top: 1px solid var(--border-subtle);
	}
	form input {
		padding: 5px 8px;
		border: 1px solid var(--border);
		border-radius: 6px;
		background: var(--surface-1);
		color: var(--text);
		font-family: var(--mono);
		font-size: 12px;
	}
	form button {
		justify-self: start;
		padding: 4px 12px;
		border: 1px solid var(--border);
		border-radius: 6px;
		background: var(--surface-1);
	}
	.hint {
		margin: 0;
		color: var(--text-muted);
	}
	code {
		font-family: var(--mono);
		font-size: 11.5px;
	}
	.link {
		padding: 0;
		border: none;
		background: none;
		color: var(--accent-strong);
		font-size: 12px;
	}
	.link:disabled {
		color: var(--text-muted);
	}
</style>
