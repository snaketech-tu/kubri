<script lang="ts">
	import type { Snippet } from 'svelte';
	import { tick, untrack } from 'svelte';
	import type { SvelteSet } from 'svelte/reactivity';
	import type { Row } from '../api';
	import { t } from '../i18n';
	import { limitShare, limitSuffix, limitsHint } from '../limits';
	import { cellText, headerHint, isNameColumn, NOTHING, pressure, shownLabel, statusIcon, strains, tone, usageColumn, type CellSource, type DisplayColumn } from '../list-cells';
	import { app } from '../state.svelte';
	import type { SortState } from '../table';
	import Icon from './Icon.svelte';
	import RowSpark from './RowSpark.svelte';

	let {
		rows,
		columns,
		cells,
		cursor,
		selection,
		sort,
		nameMax,
		markWidth,
		stale,
		holding,
		checkWidth = $bindable(0),
		room = $bindable(0),
		onpick,
		onenter,
		onopen,
		onmark,
		onmenu,
		onsort,
		after
	}: {
		rows: Row[];
		/** The columns drawn, after those that do not fit are left out. */
		columns: DisplayColumn[];
		cells: CellSource;
		/** The index of the row under the cursor. */
		cursor: number;
		selection: SvelteSet<string>;
		sort: SortState;
		/** How wide a name may be before it is cut short, when the columns are fitted. */
		nameMax: number | null;
		markWidth: number;
		/** The rows are from before the list was left, until the cluster sends them again. */
		stale: boolean;
		/** Waiting a moment for the first metrics, so the columns are drawn once. */
		holding: boolean;
		/** The mark column's width, where the name sticks when the list scrolls sideways. */
		checkWidth?: number;
		/** The width the table has, to leave out the columns that matter least when it is narrow. */
		room?: number;
		onpick: (row: Row) => void;
		onenter: (row: Row) => void;
		onopen: (row: Row) => void;
		onmark: (row: Row) => void;
		onmenu: (row: Row, x: number, y: number, align?: 'left' | 'right', column?: DisplayColumn) => void;
		onsort: (index: number) => void;
		/** What follows the table, such as a word when it is empty. */
		after?: Snippet;
	} = $props();

	let scroller = $state<HTMLDivElement>();

	// Long lists render only the rows in view and a few around them, so 10,000 pods stay quick.
	const WINDOWED_FROM = 300;
	const OVERSCAN = 20;
	let scrollTop = $state(0);
	let viewHeight = $state(800);
	let rowHeight = $state(41);
	const windowed = $derived(rows.length > WINDOWED_FROM);
	const first = $derived(windowed ? Math.max(0, Math.floor(scrollTop / rowHeight) - OVERSCAN) : 0);
	const last = $derived(
		windowed ? Math.min(rows.length, Math.ceil((scrollTop + viewHeight) / rowHeight) + OVERSCAN) : rows.length
	);
	const shown = $derived(holding ? [] : windowed ? rows.slice(first, last) : rows);

	// Rows are as high as the zoom and font make them.
	$effect(() => {
		if (!shown.length) return;
		const height = scroller?.querySelector('tbody tr.row')?.getBoundingClientRect().height;
		if (height) untrack(() => (rowHeight = height));
	});

	/** Scrolls the row under the cursor into view, so a command never acts on a row out of sight. */
	export function revealCursor() {
		if (windowed && scroller) {
			const head = scroller.querySelector('thead')?.getBoundingClientRect().height ?? 0;
			const top = cursor * rowHeight;
			if (top < scroller.scrollTop) scroller.scrollTop = top;
			else if (top + rowHeight > scroller.scrollTop + scroller.clientHeight - head)
				scroller.scrollTop = top + rowHeight - scroller.clientHeight + head;
		}
		return tick().then(() => scroller?.querySelector('tr.cursor')?.scrollIntoView({ block: 'nearest' }));
	}
</script>

<div
	class="table-wrap"
	bind:this={scroller}
	bind:clientHeight={viewHeight}
	bind:clientWidth={room}
	onscroll={() => (scrollTop = scroller?.scrollTop ?? 0)}
>
	<table class:stale aria-rowcount={windowed ? rows.length + 1 : undefined} style:--check-width="{markWidth}px">
		<thead>
			<tr>
				<th class="check" bind:offsetWidth={checkWidth}></th>
				{#each columns as column (column.label + column.index)}
					<th
						class:numeric={column.kind === 'metric'}
						class:name={isNameColumn(column)}
						class:sorted={sort.column === column.index}
						aria-sort={sort.column === column.index ? (sort.descending ? 'descending' : 'ascending') : 'none'}
					>
						<button class="sort" onclick={() => onsort(column.index)} title={headerHint(column)}>
							{shownLabel(column)}
							<span class="arrow" aria-hidden="true">
								{sort.column === column.index ? (sort.descending ? '↓' : '↑') : '↕'}
							</span>
						</button>
					</th>
				{/each}
				<th class="more" aria-label={t('Actions')}></th>
			</tr>
		</thead>
		<tbody>
			{#if first > 0}
				<tr class="spacer" aria-hidden="true" style:height="{first * rowHeight}px"><td colspan={columns.length + 2}></td></tr>
			{/if}
			{#each shown as row, j (row.key)}
				{@const i = first + j}
				<tr
					class="row"
					aria-rowindex={windowed ? i + 2 : undefined}
					class:cursor={i === cursor}
					class:checked={selection.has(row.key)}
					onclick={() => onpick(row)}
					ondblclick={() => onenter(row)}
					oncontextmenu={(e) => {
						e.preventDefault();
						onmenu(row, e.clientX, e.clientY);
					}}
					onmousedown={(e) => {
						// A double click opens the row; it should not also select a word. Dragging still selects.
						if (e.detail > 1) e.preventDefault();
					}}
				>
					<td class="check">
						<input
							type="checkbox"
							aria-label={t('Mark {name}', { name: row.name })}
							checked={selection.has(row.key)}
							onclick={(e) => {
								e.stopPropagation();
								onmark(row);
							}}
						/>
					</td>
					{#each columns as column (column.label + column.index)}
						{@const raw = cellText(cells, row, column)}
						{@const text = column.kind === 'cell' && !column.name && NOTHING.test(raw) ? '—' : raw}
						{@const color = tone(cells, row, column, raw)}
						{@const measure = usageColumn(cells, column)}
						{@const usage = measure ? cells.usage(row.namespace, row.name) : undefined}
						{@const hint = column.kind === 'metric' && (column.label === 'CPU' || column.label === 'Mem')
							? [pressure(cells, row, column.label === 'CPU' ? 'cpu' : 'memory')?.hint, measure && limitsHint(usage, measure)].filter(Boolean).join('\n') || undefined
							: undefined}
						{@const icon = statusIcon(column, color)}
						<td
							oncontextmenu={(e) => {
								e.preventDefault();
								e.stopPropagation();
								onmenu(row, e.clientX, e.clientY, 'left', column);
							}}
							class={color}
							class:wrap={column.kind === 'cell' && column.label === 'Message'}
							title={hint ??
								(column.kind === 'cell' && column.label === 'Schedule' && cells.resource.kind === 'CronJob'
									? String(row.cells[column.index] ?? '')
									: undefined)}
							class:name={column.kind === 'cell' && column.name}
							style:max-width={isNameColumn(column) && nameMax ? `${nameMax}px` : null}
							class:muted={column.kind === 'namespace' || (column.kind === 'cell' && column.age)}
							class:numeric={column.kind === 'metric'}
							class:limited={measure && limitSuffix(usage, measure)}
						>
							{#if column.kind === 'production'}
								{@const rule = app.productionRuleOf(row.name, row.labels)}
								{@const byRule = !!rule && rule.by !== 'name'}
								<button
									class="production"
									class:on={!!rule}
									class:ruled={byRule}
									aria-pressed={!!rule}
									title={byRule
										? t('Production by the rule {rule}. Change it in the context settings.', { rule: rule!.rule })
										: rule
											? t('Production: a red marker, typed names before deleting, and no changes by AI agents. Click to unmark.')
											: t('Mark {name} as production', { name: row.name })}
									onclick={(e) => {
										e.stopPropagation();
										if (byRule) {
											app.toast(t('Production by the rule {rule}. Change it in the context settings.', { rule: rule!.rule }));
											return;
										}
										const names = selection.has(row.key) && selection.size > 1 ? rows.filter((r) => selection.has(r.key)).map((r) => r.name) : [row.name];
										void app.setProductionNamespace(names, !rule);
									}}
									ondblclick={(e) => e.stopPropagation()}
								>
									<Icon name="shield" size={13} />
									{#if rule}<span>{rule.by === 'name' ? t('Production') : rule.rule}</span>{:else}<span class="mark">{t('Mark')}</span>{/if}
								</button>
							{:else if column.kind === 'cell' && column.name}
								<button
									class="open"
									title={t('Describe {name}', { name: row.name })}
									onclick={(e) => {
										e.stopPropagation();
										onopen(row);
									}}>{text}</button
								>
							{:else if icon}
								<span class="status"
									><Icon name={icon} size={15} />{text}{#each strains(cells, row) as strain (strain.label)}<span class="strain" title={strain.hint}
											>{strain.label}</span
										>{/each}</span
								>
							{:else if column.kind === 'cell' && column.label === 'Message'}
								<span class="clamp" title={text}>{text}</span>
							{:else if column.kind === 'metric' && (column.label === 'CPU' || column.label === 'Mem')}
								{@const trail = cells.trail(row.namespace, row.name)}
								{#if trail}<RowSpark
										values={column.label === 'CPU' ? trail.cpu : trail.memory}
										label={t('{column} of {name}', { column: column.label, name: row.name })}
									/>{/if}<span class="reading">{text}</span>{#if measure && limitSuffix(usage, measure)}
									{@const share = limitShare(usage, measure) ?? 0}
									<span class="limit">{limitSuffix(usage, measure)}</span><span
										class="fill"
										class:full={share >= 0.9}
										style:--share="{Math.round(share * 100)}%"
										aria-hidden="true"
									></span>
								{/if}
							{:else}
								{text}
							{/if}
						</td>
					{/each}
					<td class="more">
						<button
							aria-label={t('Actions for {name}', { name: row.name })}
							title={t('Actions (right click)')}
							onclick={(e) => {
								e.stopPropagation();
								const rect = e.currentTarget.getBoundingClientRect();
								onmenu(row, rect.right, rect.bottom + 4, 'right');
							}}
							ondblclick={(e) => e.stopPropagation()}>⋯</button
						>
					</td>
				</tr>
			{/each}
			{#if last < rows.length}
				<tr class="spacer" aria-hidden="true" style:height="{(rows.length - last) * rowHeight}px"
					><td colspan={columns.length + 2}></td></tr
				>
			{/if}
		</tbody>
	</table>
	{@render after?.()}
</div>

<style>
	button.production {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		max-width: 100%;
		padding: 2px 9px 2px 7px;
		border: 1px solid transparent;
		border-radius: 999px;
		background: none;
		color: var(--text-muted);
		font-size: 12px;
		font-weight: 500;
		white-space: nowrap;
	}
	button.production span {
		overflow: hidden;
		text-overflow: ellipsis;
	}
	/* Unmarked rows show only the shield until the row is pointed at, so the column stays calm. */
	button.production .mark {
		opacity: 0;
	}
	tr:hover button.production .mark,
	tr.cursor button.production .mark,
	button.production:focus-visible .mark {
		opacity: 1;
	}
	button.production:not(.on):hover {
		border-color: var(--border);
		color: var(--text);
	}
	button.production.on {
		background: color-mix(in srgb, var(--bad) 12%, var(--surface-1));
		color: var(--bad);
	}
	button.production.ruled {
		background: none;
		border-color: color-mix(in srgb, var(--bad) 30%, var(--border));
	}
	button.production.on:not(.ruled):hover {
		border-color: color-mix(in srgb, var(--bad) 40%, transparent);
	}
	table.stale tbody {
		opacity: 0.55;
	}
	.reading {
		display: inline-block;
		min-width: 4.5ch;
		text-align: right;
	}
	/* What the pod may use at most, after its use, and how full that is as a thin bar. */
	.limit {
		margin-left: 0.4ch;
		color: var(--text-muted);
		font-size: 0.92em;
	}
	td.limited {
		position: relative;
	}
	.fill {
		position: absolute;
		left: 8px;
		right: 8px;
		bottom: 5px;
		height: 2px;
		border-radius: 1px;
		background: linear-gradient(to right, var(--fill-color) var(--share), var(--border-subtle) var(--share));
		--fill-color: color-mix(in srgb, var(--text-muted) 70%, transparent);
	}
	.fill.full {
		--fill-color: var(--bad);
	}
	.table-wrap {
		flex: 1;
		min-height: 0;
		overflow: auto;
	}
	table {
		width: 100%;
		border-collapse: collapse;
		font-size: 13px;
	}
	th {
		position: sticky;
		top: 0;
		z-index: 1;
		padding: 0 8px;
		background: var(--surface-1);
		border-bottom: 1px solid var(--border);
		text-align: left;
		font-size: 12px;
		font-weight: 600;
		color: var(--text-muted);
		white-space: nowrap;
	}
	.sort {
		display: inline-flex;
		align-items: center;
		gap: 3px;
		padding: 9px 0;
		border: none;
		background: none;
		color: inherit;
		font: inherit;
	}
	th:hover,
	th.sorted {
		color: var(--text);
	}
	.arrow {
		font-size: 10px;
		opacity: 0.45;
	}
	th.sorted .arrow {
		opacity: 1;
	}
	th.numeric .sort {
		flex-direction: row-reverse;
	}
	.spacer td {
		height: auto;
		padding: 0;
		border: none;
	}
	td {
		height: 40px;
		padding: 4px 8px;
		border-bottom: 1px solid var(--border-subtle);
		white-space: nowrap;
		max-width: 420px;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.check {
		width: 34px;
		padding: 0 0 0 14px;
	}
	/* The marks stay in sight, quieter until the row is pointed at, chosen or marked. */
	.check input {
		opacity: 0.45;
		margin: 0;
	}
	.check input:focus-visible {
		opacity: 1;
	}
	.more {
		width: 40px;
		padding: 2px 10px 2px 6px;
		text-align: right;
	}
	.more button {
		width: 26px;
		height: 24px;
		padding: 0;
		border: 1px solid transparent;
		border-radius: 6px;
		background: none;
		color: var(--text-muted);
		font-size: 15px;
		line-height: 1;
		opacity: 0;
	}
	tr:hover .more button,
	tr.cursor .more button,
	.more button:focus-visible {
		opacity: 1;
	}
	.more button:hover {
		border-color: var(--border);
		background: var(--surface-2);
		color: var(--text);
	}
	tr:hover .check input,
	tr.cursor .check input,
	tr.checked .check input {
		opacity: 1;
	}
	tbody tr {
		cursor: default;
	}
	tbody tr.row:hover {
		background: color-mix(in srgb, var(--surface-2) 55%, transparent);
	}
	tbody tr.checked {
		background: color-mix(in srgb, var(--accent) 7%, transparent);
	}
	tbody tr.cursor {
		background: color-mix(in srgb, var(--accent) 10%, var(--surface-1));
	}
	/* The mark and the name stay in view when the list scrolls sideways, so each row keeps its name. */
	.check,
	th.name,
	td.name {
		position: sticky;
		z-index: 1;
		background: var(--surface-1);
	}
	.check {
		left: 0;
	}
	th.name,
	td.name {
		left: var(--check-width, 34px);
	}
	th.check,
	th.name {
		z-index: 2;
	}
	tbody tr.row:hover :is(td.check, td.name) {
		background: color-mix(in srgb, var(--surface-2) 55%, var(--surface-1));
	}
	tbody tr.checked :is(td.check, td.name) {
		background: color-mix(in srgb, var(--accent) 7%, var(--surface-1));
	}
	tbody tr.cursor :is(td.check, td.name) {
		background: color-mix(in srgb, var(--accent) 10%, var(--surface-1));
	}
	tbody tr.cursor td.check {
		box-shadow: inset 3px 0 0 var(--accent);
	}
	.open {
		max-width: 100%;
		padding: 0;
		border: none;
		background: none;
		color: var(--text);
		font: inherit;
		font-weight: 600;
		text-align: left;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.open:hover {
		color: var(--accent-strong);
		text-decoration: underline;
	}
	td.name {
		font-family: var(--mono);
		font-size: 12.5px;
	}
	/* Event messages take two lines instead of one long line; the title has all of it. */
	td.wrap {
		min-width: 260px;
		max-width: 520px;
	}
	td.wrap .clamp {
		white-space: normal;
		overflow: hidden;
		display: -webkit-box;
		-webkit-line-clamp: 2;
		line-clamp: 2;
		-webkit-box-orient: vertical;
		line-height: 1.3;
	}
	.status {
		display: inline-flex;
		align-items: center;
		gap: 6px;
	}
	.strain {
		padding: 0 6px;
		border-radius: 999px;
		background: color-mix(in srgb, var(--bad) 14%, transparent);
		color: var(--bad);
		font-size: 11.5px;
		font-weight: 600;
		white-space: nowrap;
	}
	.muted {
		color: var(--text-muted);
	}
	.numeric {
		text-align: right;
		font-variant-numeric: tabular-nums;
	}
	.ok {
		color: var(--ok);
	}
	.warn {
		color: var(--warn);
	}
	.bad {
		color: var(--bad);
	}
</style>
