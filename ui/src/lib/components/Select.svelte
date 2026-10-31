<script lang="ts" generics="T extends string | number">
	import { tick } from 'svelte';
	import type { HTMLButtonAttributes } from 'svelte/elements';
	import Icon from './Icon.svelte';
	import { edge, filterOptions, step, typeahead, type SelectOption } from '../select';

	/*
	 * A select box drawn by Kubri instead of the system: native ones take the GTK theme in the Linux app,
	 * and their lists cannot be styled anywhere. It follows the select-only combobox pattern, so it reads
	 * and works from the keyboard like a native one. The list opens in the top layer, above dialogs.
	 */
	let {
		value = $bindable(),
		options,
		onchange,
		placeholder = '',
		disabled = false,
		compact = false,
		quiet = false,
		mono = false,
		searchable,
		...rest
	}: {
		value?: T;
		options: SelectOption<T>[];
		/** Called after the user picks another option. */
		onchange?: (value: T) => void;
		/** Shown while the value matches no option. */
		placeholder?: string;
		disabled?: boolean;
		/** Smaller, for toolbars. */
		compact?: boolean;
		/** Without a border or background, for selects that sit inside another frame. */
		quiet?: boolean;
		/** In the monospace font, for names and fields. */
		mono?: boolean;
		/** Puts a filter field above the list; on by default for long lists. */
		searchable?: boolean;
	} & Omit<HTMLButtonAttributes, 'value' | 'onchange' | 'disabled'> = $props();

	const id = $props.id();
	const canPopover = typeof HTMLElement !== 'undefined' && 'showPopover' in HTMLElement.prototype;

	let trigger = $state<HTMLButtonElement>();
	let popup = $state<HTMLDivElement>();
	let filterInput = $state<HTMLInputElement>();
	let open = $state(false);
	let active = $state(-1);
	let query = $state('');
	let place = $state({ left: 0, top: 0 as number | null, bottom: null as number | null, minWidth: 0, maxHeight: 320 });

	const withFilter = $derived(searchable ?? options.length > 12);
	const selected = $derived(options.findIndex((o) => o.value === value));
	const shown = $derived(withFilter ? filterOptions(options, query) : options.map((_, i) => i));
	const label = $derived(selected >= 0 ? options[selected].label : value !== undefined && value !== '' && !placeholder ? String(value) : placeholder);

	// Letters typed in a row, as native selects collect them to jump to an option.
	let typed = '';
	let typedAt = 0;
	// Escape closes the list; this keeps the same press from also closing a dialog around it.
	let swallowCancel = false;

	function optionId(i: number) {
		return `${id}-option-${i}`;
	}

	async function show(text = '') {
		if (disabled || open) return;
		query = text;
		active = selected >= 0 && !options[selected].disabled ? selected : edge(options, shown, false);
		open = true;
		await tick();
		if (!popup) return;
		if (canPopover) popup.showPopover();
		position();
		if (withFilter) filterInput?.focus();
		reveal();
	}

	function hide(refocus = true) {
		if (!open) return;
		if (canPopover && popup?.matches(':popover-open')) popup.hidePopover();
		open = false;
		query = '';
		if (refocus) trigger?.focus();
	}

	function choose(i: number) {
		const option = options[i];
		if (!option || option.disabled) return;
		hide();
		if (option.value === value) return;
		value = option.value;
		onchange?.(option.value);
	}

	/** Puts the list under the box, or above it when there is more room there. */
	function position() {
		if (!trigger || !popup) return;
		const box = trigger.getBoundingClientRect();
		const below = window.innerHeight - box.bottom - 12;
		const above = box.top - 12;
		const up = below < Math.min(240, popup.scrollHeight) && above > below;
		const width = Math.max(popup.offsetWidth, box.width);
		place = {
			left: Math.max(8, Math.min(box.left, window.innerWidth - width - 8)),
			top: up ? null : box.bottom + 4,
			bottom: up ? window.innerHeight - box.top + 4 : null,
			minWidth: box.width,
			maxHeight: Math.max(120, Math.min(320, up ? above : below))
		};
	}

	async function reveal() {
		await tick();
		popup?.querySelector(`#${CSS.escape(optionId(active))}`)?.scrollIntoView({ block: 'nearest' });
	}

	function move(to: number) {
		if (to < 0) return;
		active = to;
		reveal();
	}

	/** Moves to the option the typed letters name; returns it, or -1. */
	function seek(key: string, from: number) {
		const now = Date.now();
		typed = now - typedAt > 700 ? key : typed + key;
		typedAt = now;
		return typeahead(options, typed, from);
	}

	function printable(e: KeyboardEvent) {
		return e.key.length === 1 && !e.ctrlKey && !e.metaKey && !e.altKey;
	}

	/** Keys that work the same in the box and in the filter field while the list is open. */
	function navigate(e: KeyboardEvent): boolean {
		const page = Math.max(1, Math.floor(place.maxHeight / 30));
		switch (e.key) {
			case 'ArrowDown':
				move(e.altKey ? active : step(options, shown, active, 1));
				return true;
			case 'ArrowUp':
				if (e.altKey) choose(active);
				else move(step(options, shown, active, -1));
				return true;
			case 'PageDown':
				move(step(options, shown, active, page));
				return true;
			case 'PageUp':
				move(step(options, shown, active, -page));
				return true;
			case 'Enter':
				choose(active);
				return true;
			case 'Escape':
				swallowCancel = true;
				setTimeout(() => (swallowCancel = false));
				hide();
				return true;
			case 'Tab':
				// Back to the box first, so Tab goes on from there once the filter field is gone.
				hide();
				return false;
		}
		return false;
	}

	function onkeydown(e: KeyboardEvent) {
		if (disabled) return;
		let handled = true;
		if (!open) {
			if (e.key === 'ArrowDown' || e.key === 'ArrowUp' || e.key === 'Enter' || e.key === ' ') show();
			else if (printable(e)) {
				if (withFilter) show(e.key);
				else {
					// Like a native select: typing picks the option without opening the list.
					const i = seek(e.key, selected);
					if (i >= 0) choose(i);
				}
			} else handled = false;
		} else if (e.key === 'Home' || e.key === 'End') move(edge(options, shown, e.key === 'End'));
		else if (e.key === ' ' && !typed) choose(active);
		else if (printable(e)) move(seek(e.key, active));
		else handled = navigate(e);
		if (!printable(e)) typed = '';
		if (handled) {
			e.preventDefault();
			e.stopPropagation();
		}
	}

	function onfilterkey(e: KeyboardEvent) {
		if (navigate(e)) {
			e.preventDefault();
			e.stopPropagation();
		} else if (e.key !== 'Tab') e.stopPropagation();
	}

	function onfilter() {
		const first = edge(options, shown, false);
		if (!shown.includes(active) || options[active]?.disabled) active = first;
		reveal();
	}

	// Close on a press anywhere else, follow the box when the page scrolls, and close when the window
	// changes size. While open, Escape belongs to the list and not to a dialog around it.
	$effect(() => {
		if (!open) return;
		const away = (e: PointerEvent) => {
			const target = e.target as Node;
			// The label around the box hands its click on to the box, which then closes the list itself.
			const label = trigger?.closest('label');
			if (!label?.contains(target) && !trigger?.contains(target) && !popup?.contains(target)) hide(false);
		};
		const scrolled = (e: Event) => {
			if (!popup?.contains(e.target as Node)) position();
		};
		const resized = () => hide(false);
		document.addEventListener('pointerdown', away, true);
		document.addEventListener('scroll', scrolled, true);
		window.addEventListener('resize', resized);
		return () => {
			document.removeEventListener('pointerdown', away, true);
			document.removeEventListener('scroll', scrolled, true);
			window.removeEventListener('resize', resized);
		};
	});

	$effect(() => {
		const dialog = trigger?.closest('dialog');
		if (!dialog) return;
		const cancel = (e: Event) => {
			if (open || swallowCancel) e.preventDefault();
		};
		dialog.addEventListener('cancel', cancel);
		return () => dialog.removeEventListener('cancel', cancel);
	});

	// A select that turns disabled while open closes, as native ones do.
	$effect(() => {
		if (disabled) hide(false);
	});
</script>

<button
	{...rest}
	bind:this={trigger}
	type="button"
	class={['select', rest.class, { compact, quiet, mono, open, placeholder: selected < 0 && !!placeholder }]}
	role="combobox"
	aria-haspopup="listbox"
	aria-expanded={open}
	aria-controls={open ? `${id}-list` : undefined}
	aria-activedescendant={open && !withFilter && active >= 0 ? optionId(active) : undefined}
	{disabled}
	onclick={() => (open ? hide() : show())}
	{onkeydown}
>
	<span class="value">{label}</span>
	<span class="chevron"><Icon name="chevron" size={compact ? 14 : 15} /></span>
</button>

{#if open}
	<!-- The list keeps focus on the box, and inside a label its clicks would reach the box a second time. -->
	<div
		bind:this={popup}
		class="popup"
		role="presentation"
		popover={canPopover ? 'manual' : undefined}
		style:left="{place.left}px"
		style:top={place.top === null ? 'auto' : `${place.top}px`}
		style:bottom={place.bottom === null ? 'auto' : `${place.bottom}px`}
		style:min-width="{place.minWidth}px"
		style:max-height="{place.maxHeight}px"
		onmousedown={(e) => e.target !== filterInput && e.preventDefault()}
		onclick={(e) => e.preventDefault()}
	>
		{#if withFilter}
			<div class="filter">
				<Icon name="search" size={14} />
				<input
					bind:this={filterInput}
					bind:value={query}
					oninput={onfilter}
					onkeydown={onfilterkey}
					role="combobox"
					aria-expanded="true"
					aria-controls="{id}-list"
					aria-autocomplete="list"
					aria-activedescendant={active >= 0 ? optionId(active) : undefined}
					aria-label="Filter the choices"
					placeholder="Filter…"
					spellcheck="false"
				/>
			</div>
		{/if}
		<ul id="{id}-list" role="listbox" aria-label={rest['aria-label'] ?? rest.title ?? undefined}>
			{#each shown as i, n (i)}
				{@const option = options[i]}
				{#if option.group && option.group !== options[shown[n - 1]]?.group}
					<li class="group" role="presentation">{option.group}</li>
				{/if}
				<!-- Options take no focus; the box or filter field keeps it and points at the active one. -->
				<!-- svelte-ignore a11y_click_events_have_key_events -->
				<li
					id={optionId(i)}
					role="option"
					class:active={i === active}
					class:grouped={!!option.group}
					aria-selected={i === selected}
					aria-disabled={option.disabled || undefined}
					onpointermove={() => !option.disabled && active !== i && (active = i)}
					onclick={() => choose(i)}
				>
					<span class="label" class:mono>{option.label}</span>
					{#if i === selected}<span class="tick"><Icon name="check" size={14} /></span>{/if}
				</li>
			{:else}
				<li class="empty" role="presentation">{query ? 'Nothing matches' : 'Nothing to choose'}</li>
			{/each}
		</ul>
	</div>
{/if}

<style>
	.select {
		display: inline-flex;
		align-items: center;
		gap: 8px;
		min-width: 0;
		max-width: 100%;
		padding: 6px 8px 6px 10px;
		border: 1px solid var(--border);
		border-radius: 7px;
		background: var(--surface-1);
		color: var(--text);
		font-size: 13px;
		line-height: 1.35;
		text-align: left;
		transition:
			border-color 0.12s,
			box-shadow 0.12s,
			background 0.12s;
	}
	.select:hover:not(:disabled) {
		border-color: color-mix(in srgb, var(--text-muted) 55%, var(--border));
	}
	.select:focus-visible,
	.select.open {
		outline: none;
		border-color: var(--accent);
		box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 20%, transparent);
	}
	.select.compact {
		gap: 6px;
		padding: 4px 6px 4px 9px;
		border-radius: 6px;
		font-size: 12.5px;
	}
	.select.quiet {
		border-color: transparent;
		background: none;
		box-shadow: none;
	}
	.select.quiet:hover:not(:disabled) {
		border-color: transparent;
	}
	.select.quiet:focus-visible {
		box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent) 45%, transparent);
	}
	.mono {
		font-family: var(--mono);
	}
	.value {
		flex: 1;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.placeholder .value {
		color: var(--text-muted);
	}
	.chevron {
		display: flex;
		flex: none;
		color: var(--text-muted);
		transition: transform 0.15s;
	}
	.open .chevron {
		transform: rotate(180deg);
		color: var(--accent-strong);
	}

	.popup {
		position: fixed;
		inset: auto;
		z-index: 60;
		display: flex;
		flex-direction: column;
		max-width: min(440px, calc(100vw - 16px));
		margin: 0;
		padding: 0;
		overflow: hidden;
		border: 1px solid var(--border);
		border-radius: 10px;
		background: var(--surface-1);
		color: var(--text);
		box-shadow: var(--shadow);
		font-size: 13px;
		animation: drop 0.12s ease-out;
	}
	@keyframes drop {
		from {
			opacity: 0;
			transform: translateY(-3px);
		}
	}
	.filter {
		display: flex;
		align-items: center;
		gap: 7px;
		padding: 0 10px;
		border-bottom: 1px solid var(--border);
		color: var(--text-muted);
	}
	.filter input {
		flex: 1;
		min-width: 0;
		padding: 8px 0;
		border: none;
		border-radius: 0;
		background: none;
		color: var(--text);
		font-size: 13px;
	}
	.filter input:focus {
		box-shadow: none;
	}
	ul {
		/* Not flex: 1, whose zero basis WebKit takes literally in a list of fitted height. */
		flex: 0 1 auto;
		min-height: 0;
		margin: 0;
		padding: 4px;
		overflow-y: auto;
		list-style: none;
		outline: none;
		overscroll-behavior: contain;
	}
	li[role='option'] {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 5px 8px 5px 10px;
		border-radius: 6px;
		cursor: pointer;
		white-space: nowrap;
	}
	li.grouped {
		padding-left: 16px;
	}
	li.active {
		background: color-mix(in srgb, var(--accent) 14%, transparent);
	}
	li[aria-selected='true'] {
		color: var(--accent-strong);
		font-weight: 600;
	}
	li[aria-disabled='true'] {
		color: var(--text-muted);
		cursor: default;
		opacity: 0.7;
	}
	.label {
		flex: 1;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.tick {
		display: flex;
		flex: none;
	}
	.group {
		position: sticky;
		top: -4px;
		padding: 7px 10px 3px;
		background: var(--surface-1);
		color: var(--text-muted);
		font-size: 10.5px;
		font-weight: 600;
		letter-spacing: 0.06em;
		text-transform: uppercase;
	}
	.empty {
		padding: 8px 10px;
		color: var(--text-muted);
	}
</style>
