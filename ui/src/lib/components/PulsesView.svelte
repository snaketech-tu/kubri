<script module lang="ts">
	import type { Sample } from '../pulse';

	/** Trend lines survive leaving the page, per context and namespace. */
	const kept = new Map<string, { cpu: Sample[]; memory: Sample[] }>();
</script>

<script lang="ts">
	import { forControl } from '../keys';
	import { tick } from 'svelte';
	import { backend, type KindPulse, type Pulse, type RecentEvent, type ResourceRef, type Utilization } from '../api';
	import { age, errorMessage, memory } from '../format';
	import { fill, kindLabel, meterTone, record } from '../pulse';
	import { app, findResource } from '../state.svelte';
	import HealthIcon from './HealthIcon.svelte';
	import Sparkline from './Sparkline.svelte';

	const REFRESH_MS = 10_000;

	let pulse = $state<Pulse | null>(null);
	let error = $state<string | null>(null);
	let updated = $state(0);
	let now = $state(Date.now());
	let cpuHistory = $state.raw<Sample[]>([]);
	let memoryHistory = $state.raw<Sample[]>([]);
	let cursor = $state(0);
	let grid = $state<HTMLDivElement>();

	const namespace = $derived(app.namespace || null);

	$effect(() => {
		const ns = namespace;
		const key = `${app.cluster?.context}/${ns}`;
		let cancelled = false;
		let loading = false;
		pulse = null;
		error = null;
		cpuHistory = record(kept.get(key)?.cpu ?? [], null, Date.now());
		memoryHistory = record(kept.get(key)?.memory ?? [], null, Date.now());
		const load = async () => {
			if (loading) return;
			loading = true;
			try {
				const next = await backend.pulse(ns);
				if (cancelled) return;
				const at = Date.now();
				pulse = next;
				error = null;
				updated = at;
				cpuHistory = record(cpuHistory, next.cpu.used ?? next.cpu.requested, at);
				memoryHistory = record(memoryHistory, next.memory.used ?? next.memory.requested, at);
				kept.set(key, { cpu: cpuHistory, memory: memoryHistory });
			} catch (err) {
				if (!cancelled) error = errorMessage(err);
			} finally {
				loading = false;
			}
		};
		load();
		const refresh = setInterval(() => document.hidden || load(), REFRESH_MS);
		const clock = setInterval(() => (now = Date.now()), 1000);
		return () => {
			cancelled = true;
			clearInterval(refresh);
			clearInterval(clock);
		};
	});

	interface Tile {
		key: string;
		open: () => void;
	}

	function openList(ref: ResourceRef) {
		const resource = findResource(app.resources, ref);
		if (resource) app.open(resource);
		else app.toast(`${ref.plural} is not served by this cluster`, 'bad');
	}

	const usageList: ResourceRef = $derived(
		namespace ? { group: '', version: 'v1', plural: 'pods' } : { group: '', version: 'v1', plural: 'nodes' }
	);
	const tiles = $derived<Tile[]>([
		{ key: 'cpu', open: () => openList(usageList) },
		{ key: 'memory', open: () => openList(usageList) },
		{ key: 'events', open: () => openList({ group: '', version: 'v1', plural: 'events' }) },
		...(pulse?.kinds ?? []).map((k) => ({ key: k.kind, open: () => openList(k.resource) }))
	]);

	function openEvent(event: RecentEvent) {
		const resource = findResource(app.resources, event.object.resource);
		if (resource) app.showDetails(resource, { ...event.object, resource });
	}

	function cores(millicores: number) {
		const value = millicores / 1000;
		return value < 10 ? value.toFixed(1) : Math.round(value).toString();
	}

	const percent = (value: number) => `${Math.round(value * 100)}%`;

	function move(delta: number) {
		cursor = Math.max(0, Math.min(tiles.length - 1, cursor + delta));
		tick().then(() => grid?.querySelector<HTMLElement>(`[data-tile="${cursor}"]`)?.focus());
	}

	function onkeydown(e: KeyboardEvent) {
		if (app.overlayOpen || e.ctrlKey || e.metaKey || e.altKey) return;
		if (forControl(e)) return;
		const handled = (() => {
			switch (e.key) {
				case 'ArrowRight':
				case 'ArrowDown':
				case 'l':
				case 'j':
					move(1);
					return true;
				case 'ArrowLeft':
				case 'ArrowUp':
				case 'h':
				case 'k':
					move(-1);
					return true;
				case 'Enter':
					tiles[cursor]?.open();
					return true;
				case 'Escape':
					if (!app.details) return false;
					app.details = null;
					app.maximized = false;
					return true;
				case 'f':
					if (!app.details) return false;
					app.maximized = !app.maximized;
					return true;
			}
			return false;
		})();
		if (handled) e.preventDefault();
	}
</script>

<svelte:window {onkeydown} />

<section>
	<header>
		<h1>Pulses</h1>
		<span class="scope">{namespace ? `Namespace ${namespace}` : 'All namespaces'}</span>
		<span class="spacer"></span>
		{#if updated}
			<span class="muted" title="Refreshes every {REFRESH_MS / 1000} seconds">Updated {age(new Date(updated).toISOString(), now) || '0s'} ago</span>
		{/if}
	</header>

	{#if error}
		<div class="error">Could not load the overview: {error}</div>
	{/if}

	{#if pulse}
		<div class="body" bind:this={grid}>
			<div class="top">
				{@render usage(0, 'CPU', pulse.cpu, cpuHistory, (m) => `${cores(m)} cores`)}
				{@render usage(1, 'Memory', pulse.memory, memoryHistory, memory)}
				<button class="tile" class:cursor={cursor === 2} data-tile="2" onclick={() => ((cursor = 2), tiles[2].open())}>
					<span class="label">Warnings, last hour</span>
					{#if pulse.events.error}
						<span class="denied" title={pulse.events.error}>No access</span>
					{:else}
						<span class="value">{pulse.events.warning}</span>
						<span class="counts">
							<span>{pulse.events.normal} normal events</span>
						</span>
					{/if}
				</button>
			</div>

			<div class="kinds">
				{#each pulse.kinds as kind, i (kind.kind)}
					{@render kindTile(kind, i + 3)}
				{/each}
			</div>

			<div class="warnings">
				<h2>Recent warnings</h2>
				{#if pulse.events.recent.length}
					<table>
						<thead>
							<tr><th>Seen</th><th>Object</th><th>Reason</th><th>Message</th></tr>
						</thead>
						<tbody>
							{#each pulse.events.recent as event, i (i)}
								<tr onclick={() => openEvent(event)} title="Open {event.object.name}">
									<td class="muted">{age(event.lastSeen, now)}</td>
									<td class="object">
										<span class="kind">{event.objectKind}</span>
										{event.object.name}
										{#if !namespace && event.object.namespace}<span class="muted">· {event.object.namespace}</span>{/if}
									</td>
									<td class="reason">{event.reason}{#if event.count > 1}<span class="muted"> ×{event.count}</span>{/if}</td>
									<td class="message" title={event.message}>{event.message}</td>
								</tr>
							{/each}
						</tbody>
					</table>
				{:else}
					<p class="muted">No warnings in the last hour.</p>
				{/if}
			</div>
		</div>
	{:else if !error}
		<div class="loading">Loading the overview…</div>
	{/if}
</section>

{#snippet usage(index: number, label: string, u: Utilization, history: Sample[], format: (v: number) => string)}
	{@const level = fill(u)}
	<!-- Not a button as a whole: the trend line inside takes focus of its own. -->
	<div class="tile wide" class:cursor={cursor === index}>
		<button class="label open" data-tile={index} onclick={() => ((cursor = index), tiles[index].open())} title="Open {namespace ? 'pods' : 'nodes'}">
			{label}
			{#if level}<span class="percent">{percent(level.value)}</span>{/if}
		</button>
		{#if level && u.capacity}
			<span class="meter" role="meter" aria-valuemin="0" aria-valuemax="100" aria-valuenow={Math.round(level.value * 100)} aria-label="{label} {level.measured ? 'used' : 'requested'}">
				<span class="fill {meterTone(level.value)}" style:width={percent(Math.min(level.value, 1))}></span>
				{#if level.measured && u.requested !== null}
					<span class="mark" style:left={percent(Math.min(u.requested / u.capacity, 1))} title="Requested"></span>
				{/if}
			</span>
			<span class="facts">
				{#if level.measured && u.used !== null}
					<span><strong>{format(u.used)}</strong> used of {format(u.capacity)}</span>
					{#if u.requested !== null}<span class="muted"><span class="tick"></span>requested {format(u.requested)} · {percent(u.requested / u.capacity)}</span>{/if}
				{:else if u.requested !== null}
					<span><strong>{format(u.requested)}</strong> requested of {format(u.capacity)}</span>
					<span class="muted">Install metrics-server to see actual use</span>
				{/if}
			</span>
			<Sparkline samples={history} {format} label="{label} {level.measured ? 'used' : 'requested'}" {now} />
		{:else}
			<span class="denied">Not available</span>
			<span class="muted small">Needs access to nodes{u.used === null ? ' and metrics-server' : ''}</span>
		{/if}
	</div>
{/snippet}

{#snippet kindTile(kind: KindPulse, index: number)}
	<button class="tile" class:cursor={cursor === index} data-tile={index} onclick={() => ((cursor = index), tiles[index].open())}>
		<span class="label">{kindLabel(kind)}</span>
		{#if kind.error}
			<span class="denied" title={kind.error}>No access</span>
		{:else}
			<span class="value">{kind.total}</span>
			<span class="bar" aria-hidden="true">
				{#each [['ok', kind.ok], ['warning', kind.warning], ['failed', kind.failed]] as [health, count] (health)}
					{#if Number(count) > 0}<span class="segment {health}" style:flex-grow={count}></span>{/if}
				{/each}
			</span>
			<span class="counts">
				{#if kind.total === 0}
					<span class="muted">none</span>
				{:else}
					<span class:zero={!kind.ok}><HealthIcon health="ok" /> {kind.ok} ok</span>
					<span class:zero={!kind.warning}><HealthIcon health="warning" /> {kind.warning}</span>
					<span class:zero={!kind.failed}><HealthIcon health="failed" /> {kind.failed}</span>
				{/if}
			</span>
		{/if}
	</button>
{/snippet}

<style>
	section {
		display: flex;
		flex-direction: column;
		min-height: 0;
		min-width: 0;
	}
	header {
		display: flex;
		align-items: baseline;
		gap: 10px;
		padding: 14px 20px 10px;
	}
	h1 {
		margin: 0;
		font-size: 20px;
	}
	.scope {
		padding: 1px 8px;
		border-radius: 999px;
		background: var(--surface-2);
		font-size: 12px;
		font-weight: 600;
	}
	.spacer {
		flex: 1;
	}
	.muted {
		color: var(--text-muted);
	}
	.small {
		font-size: 11.5px;
	}
	.error {
		margin: 0 20px 10px;
		padding: 8px 12px;
		border-radius: 8px;
		background: color-mix(in srgb, var(--bad) 12%, var(--surface-1));
		color: var(--bad);
	}
	.loading {
		padding: 40px;
		text-align: center;
		color: var(--text-muted);
	}
	.body {
		flex: 1;
		overflow: auto;
		padding: 2px 20px 24px;
	}
	.top {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(260px, 1fr));
		gap: 12px;
		margin-bottom: 12px;
	}
	.kinds {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
		gap: 12px;
	}
	.tile {
		display: flex;
		flex-direction: column;
		gap: 8px;
		min-width: 0;
		padding: 14px 16px;
		border: 1px solid var(--border);
		border-radius: 10px;
		background: var(--surface-1);
		text-align: left;
		font: inherit;
		color: inherit;
	}
	.tile:hover {
		border-color: color-mix(in srgb, var(--accent) 50%, var(--border));
	}
	.tile.cursor,
	.tile:focus-visible,
	.tile:has(.open:focus-visible) {
		outline: none;
		border-color: var(--accent);
		box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 18%, transparent);
	}
	.label {
		display: flex;
		justify-content: space-between;
		font-size: 12.5px;
		font-weight: 600;
		color: var(--text-muted);
	}
	.label.open {
		padding: 0;
		border: none;
		background: none;
		text-align: left;
		font: inherit;
		font-size: 12.5px;
		font-weight: 600;
	}
	.label.open:hover {
		color: var(--accent-strong);
	}
	.label.open:focus-visible {
		outline: none;
	}
	.percent {
		color: var(--text);
		font-size: 15px;
	}
	.value {
		font-size: 28px;
		font-weight: 650;
		line-height: 1;
		letter-spacing: -0.01em;
	}
	.denied {
		font-size: 15px;
		font-weight: 600;
		color: var(--text-muted);
	}
	.bar {
		display: flex;
		gap: 2px;
		height: 8px;
		border-radius: 4px;
		overflow: hidden;
		background: var(--surface-2);
	}
	.segment {
		flex-basis: 0;
		min-width: 4px;
	}
	.segment.ok {
		background: var(--ok);
	}
	.segment.warning {
		background: var(--warn-mark);
	}
	.segment.failed {
		background: var(--bad);
	}
	.counts {
		display: flex;
		flex-wrap: wrap;
		gap: 4px 12px;
		font-size: 12px;
	}
	.counts > span {
		display: inline-flex;
		align-items: center;
		gap: 4px;
	}
	.counts .zero {
		opacity: 0.45;
	}
	.meter {
		position: relative;
		height: 10px;
		border-radius: 5px;
		background: color-mix(in srgb, var(--accent) 16%, var(--surface-2));
	}
	.fill {
		position: absolute;
		inset: 0 auto 0 0;
		border-radius: 5px;
		background: var(--accent);
	}
	.fill.warn {
		background: var(--warn-mark);
	}
	.fill.bad {
		background: var(--bad);
	}
	.mark {
		position: absolute;
		top: -3px;
		bottom: -3px;
		width: 2px;
		margin-left: -1px;
		border-radius: 1px;
		background: var(--text);
	}
	.facts {
		display: flex;
		flex-wrap: wrap;
		justify-content: space-between;
		gap: 2px 12px;
		font-size: 12px;
	}
	.tick {
		display: inline-block;
		width: 2px;
		height: 10px;
		margin-right: 5px;
		vertical-align: -1px;
		background: var(--text);
	}
	.warnings {
		margin-top: 22px;
	}
	h2 {
		margin: 0 0 8px;
		font-size: 14px;
	}
	table {
		width: 100%;
		border-collapse: collapse;
		table-layout: fixed;
		font-size: 12.5px;
	}
	th {
		padding: 6px 10px;
		border-bottom: 1px solid var(--border);
		text-align: left;
		font-size: 11px;
		font-weight: 600;
		letter-spacing: 0.05em;
		text-transform: uppercase;
		color: var(--text-muted);
	}
	th:nth-child(1) {
		width: 64px;
	}
	th:nth-child(2) {
		width: 32%;
	}
	th:nth-child(3) {
		width: 210px;
	}
	td {
		padding: 6px 10px;
		border-bottom: 1px solid var(--border-subtle);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	tbody tr {
		cursor: pointer;
	}
	tbody tr:hover {
		background: var(--surface-1);
	}
	.kind {
		margin-right: 4px;
		color: var(--text-muted);
	}
	.object {
		font-family: var(--mono);
		font-size: 12px;
	}
	.reason {
		color: var(--warn);
		font-weight: 600;
	}
</style>
