<script lang="ts">
	import type { Amounts, Capacity } from '../api';
	import { plural, t } from '../i18n';
	import {
		MEASURES,
		budget,
		formatAmount,
		ladder,
		podLimits,
		podTotals,
		step,
		type FieldProblem,
		type Measure,
		type ResourceRow
	} from '../resources';

	let {
		rows = $bindable(),
		capacity,
		capacityError,
		init,
		daemon,
		problems
	}: {
		rows: ResourceRow[];
		capacity: Capacity | null;
		capacityError: string | null;
		init: Amounts;
		daemon: boolean;
		problems: FieldProblem[];
	} = $props();

	const id = `sizes-${Math.random().toString(36).slice(2, 8)}`;
	const PARTS = ['requests', 'limits'] as const;
	const label = (part: 'requests' | 'limits', m: Measure) =>
		m === 'cpu' ? (part === 'requests' ? t('CPU request') : t('CPU limit')) : part === 'requests' ? t('Memory request') : t('Memory limit');

	const before = $derived(podTotals(rows, true, init));
	const after = $derived(podTotals(rows, false, init));
	const room = $derived(
		capacity ? budget(capacity, before, after, { before: podLimits(rows, true), after: podLimits(rows, false) }, daemon) : null
	);

	function problem(row: ResourceRow, part: 'requests' | 'limits', m: Measure) {
		return problems.find((p) => p.container === row.container && p.part === part && p.measure === m);
	}

	function onkeydown(e: KeyboardEvent, row: ResourceRow, part: 'requests' | 'limits', m: Measure) {
		if ((e.key !== 'ArrowUp' && e.key !== 'ArrowDown') || e.altKey || e.ctrlKey || e.metaKey) return;
		e.preventDefault();
		row[part][m] = step(m, row[part][m], e.key === 'ArrowUp' ? 1 : -1);
	}

	/** The share of the nodes' room a part of the bar takes, in percent. */
	const share = (value: number, total: number) => `${total > 0 ? Math.min(100, Math.max(0, (value / total) * 100)) : 0}%`;
</script>

{#each MEASURES as m (m)}
	<datalist id="{id}-{m}">
		{#each ladder(m) as size (size)}<option value={size}></option>{/each}
	</datalist>
{/each}

<div class="sizes">
	<div class="head" aria-hidden="true">
		<span>{t('Container')}</span>
		{#each MEASURES as m (m)}
			{#each PARTS as part (part)}<span>{label(part, m)}</span>{/each}
		{/each}
	</div>
	{#each rows as row (row.container)}
		<div class="row">
			<span class="name mono">{row.container}</span>
			{#each MEASURES as m (m)}
				{#each PARTS as part (part)}
					{@const wrong = problem(row, part, m)}
					{@const was = row.was[part][m]}
					<label class="cell" class:changed={row[part][m].trim() !== was}>
						<span class="cell-label">{label(part, m)}</span>
						<input
							class="mono"
							data-field="{row.container}/{part}/{m}"
							list="{id}-{m}"
							bind:value={row[part][m]}
							onkeydown={(e) => onkeydown(e, row, part, m)}
							spellcheck="false"
							autocomplete="off"
							placeholder={t('none')}
							aria-label="{row.container}: {label(part, m)}"
							aria-invalid={wrong ? 'true' : undefined}
							title={wrong?.text}
						/>
						{#if row[part][m].trim() !== was}
							<span class="was">{was ? t('was {value}', { value: was }) : t('was none')}</span>
						{/if}
					</label>
				{/each}
			{/each}
		</div>
	{/each}
</div>

{#if problems.length}
	<ul class="problems" role="alert">
		{#each problems as p (`${p.container}/${p.part}/${p.measure}`)}
			<li><span class="mono">{p.container}</span>, {label(p.part, p.measure)}: {p.text}</li>
		{/each}
	</ul>
{/if}
<p class="hint">{t('Arrow keys step through common sizes. An empty request takes the limit, as Kubernetes does.')}</p>

<section class="budget" aria-label={t('Room on the nodes')}>
	{#if capacityError}
		<p class="muted">{t('Kubri cannot show the room on the nodes: {error}', { error: capacityError })}</p>
	{:else if !capacity || !room}
		<p class="muted">{t('Looking at the room on the nodes…')}</p>
	{:else}
		<p class="summary">
			{#if capacity.nodes === null}
				{t('Kubri may not list the nodes, so it shows only the quotas.')}
			{:else}
				{daemon
					? plural(capacity.pods, 'One pod per node, on {n} node', 'One pod per node, on {n} nodes')
					: plural(capacity.pods, '{n} pod', '{n} pods')}
				·
				{plural(room.nodes, '{n} node they may run on', '{n} nodes they may run on')}
			{/if}
		</p>
		{#if capacity.nodes !== null && room.nodes}
			{#each room.measures as b (b.measure)}
				{@const total = b.others + b.after}
				<div class="measure">
					<span class="what">{b.measure === 'cpu' ? 'CPU' : t('Memory')}</span>
					<span
						class="bar"
						class:over={total > b.allocatable}
						role="img"
						aria-label={t('{used} of {total} requested', { used: formatAmount(b.measure, total), total: formatAmount(b.measure, b.allocatable) })}
					>
						<span class="others" style:width={share(b.others, b.allocatable)}></span>
						<span class="own" style:width={share(b.after, b.allocatable)}></span>
					</span>
					<span class="numbers">
						{t('{used} of {total} requested', { used: formatAmount(b.measure, total), total: formatAmount(b.measure, b.allocatable) })}
						<span class="muted">
							· {t('these pods {before}, then {after}', { before: formatAmount(b.measure, b.before), after: formatAmount(b.measure, b.after) })}
						</span>
					</span>
				</div>
			{/each}
			{#if !room.warnings.some((w) => w.level === 'bad') && Number.isFinite(room.placeable)}
				<p class="muted">
					{daemon
						? t('A pod of this size fits on every node.')
						: plural(room.placeable, 'Room for {n} pod of this size.', 'Room for {n} pods of this size.')}
				</p>
			{/if}
		{/if}
		{#each room.warnings as w (w.text)}
			<p class="warning {w.level}">{w.text}</p>
		{/each}
		{#if room.warnings.some((w) => w.level === 'bad')}
			<p class="muted">{t('You can still set it, for example before adding nodes.')}</p>
		{/if}
	{/if}
</section>

<style>
	.sizes {
		container: sizes / inline-size;
		margin: 6px 0 4px;
	}
	.head,
	.row {
		display: grid;
		grid-template-columns: minmax(6rem, 1fr) repeat(4, minmax(5.5rem, 7rem));
		gap: 6px 8px;
		align-items: start;
	}
	.head {
		padding-bottom: 4px;
		font-size: 12px;
		color: var(--text-muted);
	}
	.row {
		padding: 6px 0;
		border-top: 1px solid var(--border);
	}
	.name {
		padding-top: 6px;
		font-size: 12.5px;
		overflow-wrap: anywhere;
	}
	.cell {
		display: flex;
		flex-direction: column;
		gap: 3px;
		min-width: 0;
	}
	.cell input {
		width: 100%;
		min-width: 0;
		box-sizing: border-box;
	}
	.cell.changed input {
		border-color: var(--accent);
	}
	.cell input[aria-invalid='true'] {
		border-color: var(--bad);
	}
	.cell-label {
		display: none;
		font-size: 12px;
		color: var(--text-muted);
	}
	.was {
		font-size: 11.5px;
		color: var(--text-muted);
	}
	/* Narrow: each container gets its own block, with a label on every field. */
	@container sizes (max-width: 520px) {
		.head {
			display: none;
		}
		.row {
			grid-template-columns: 1fr 1fr;
		}
		.name {
			grid-column: 1 / -1;
			padding-top: 2px;
			font-weight: 600;
		}
		.cell-label {
			display: block;
		}
	}
	.problems {
		margin: 6px 0;
		padding-left: 18px;
		color: var(--bad);
		font-size: 12.5px;
	}
	.hint {
		margin: 6px 0 12px;
		font-size: 12px;
		color: var(--text-muted);
	}
	.budget {
		padding: 10px 12px;
		border: 1px solid var(--border);
		border-radius: 8px;
		background: var(--bg);
		font-size: 12.5px;
	}
	.budget p {
		margin: 0 0 8px;
	}
	.budget p:last-child {
		margin-bottom: 0;
	}
	.summary {
		color: var(--text-muted);
	}
	.measure {
		display: grid;
		grid-template-columns: 4.5rem minmax(4rem, 1fr) auto;
		gap: 4px 10px;
		align-items: center;
		margin-bottom: 8px;
	}
	.what {
		font-weight: 600;
	}
	.bar {
		display: flex;
		height: 8px;
		overflow: hidden;
		border-radius: 4px;
		background: var(--surface-2, var(--border));
	}
	.others {
		background: var(--text-muted);
		opacity: 0.45;
	}
	.own {
		background: var(--accent);
	}
	.bar.over .own {
		background: var(--bad);
	}
	.numbers {
		font-variant-numeric: tabular-nums;
	}
	@container sizes (max-width: 520px) {
		.measure {
			grid-template-columns: 4.5rem 1fr;
		}
		.numbers {
			grid-column: 1 / -1;
		}
	}
	.warning {
		padding-left: 8px;
		border-left: 3px solid;
	}
	.warning.bad {
		border-color: var(--bad);
		color: var(--bad);
	}
	.warning.warn {
		border-color: var(--warn-mark);
		color: var(--warn);
	}
	.muted {
		color: var(--text-muted);
	}
</style>
