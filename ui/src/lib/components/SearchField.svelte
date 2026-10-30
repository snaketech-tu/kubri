<script lang="ts">
	import Icon from './Icon.svelte';

	let {
		value = $bindable(''),
		input = $bindable(),
		label,
		placeholder = 'Filter…',
		key = '/'
	}: {
		value?: string;
		input?: HTMLInputElement;
		/** Read by screen readers; the placeholder is not a label. */
		label: string;
		placeholder?: string;
		/** The shortcut that focuses the field. */
		key?: string;
	} = $props();
</script>

<label class="search">
	<Icon name="search" size={16} />
	<!-- Esc leaves the field so the page's keys work again; views that also clear it do that themselves. -->
	<input bind:this={input} bind:value aria-label={label} {placeholder} onkeydown={(e) => e.key === 'Escape' && e.currentTarget.blur()} />
	{#if key}<kbd aria-hidden="true">{key}</kbd>{/if}
</label>

<style>
	.search {
		display: flex;
		align-items: center;
		gap: 8px;
		min-width: 0;
		padding: 0 8px 0 11px;
		border: 1px solid var(--border);
		border-radius: 8px;
		background: var(--surface-1);
		color: var(--text-muted);
	}
	.search:focus-within {
		border-color: var(--accent);
		box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 20%, transparent);
	}
	input {
		flex: 1;
		min-width: 0;
		padding: 7px 0;
		border: none;
		border-radius: 0;
		background: none;
		color: var(--text);
		font-size: 13px;
	}
	input:focus {
		box-shadow: none;
	}
	kbd {
		padding: 0 5px;
		border: 1px solid var(--border);
		border-radius: 4px;
		font-family: var(--mono);
		font-size: 11px;
	}
</style>
