<script module lang="ts">
	import type { Sample } from '../pulse';

	/** Trend lines survive leaving the page, per context and namespace. */
	const kept = new Map<string, { cpu: Sample[]; memory: Sample[] }>();
</script>

<script lang="ts">
	import { forControl } from '../keys';
	import { tick } from 'svelte';
	import { backend, type CertificateAlert, type KindPulse, type Pulse, type RecentEvent, type ResourceRef, type Utilization } from '../api';
	import { age, errorMessage, memory, relative } from '../format';
	import { attentionItems, fill, kindLabel, meterTone, record, shortMessage, splitKinds } from '../pulse';
	import { app, findResource } from '../state.svelte';
	import Icon, { type IconName } from './Icon.svelte';
	import Sparkline from './Sparkline.svelte';

	const REFRESH_MS = 10_000;
	/** Recent warnings the overview lists; the events list has them all. */
	const RECENT = 6;

	let pulse = $state<Pulse | null>(null);
	let error = $state<string | null>(null);
	let updated = $state(0);
	let now = $state(Date.now());
	let cpuHistory = $state.raw<Sample[]>([]);
	let memoryHistory = $state.raw<Sample[]>([]);
	/** The last hour from Prometheus, when it is there; live samples continue it. */
	let stored = $state.raw<{ cpu: Sample[]; memory: Sample[] } | null>(null);
	let cursor = $state(0);
	/** The cursor shows once the keys move it, not on a mouse user's first look. */
	let keyboard = $state(false);
	let body = $state<HTMLDivElement>();

	const namespace = $derived(app.namespace || null);
	const HOUR = 3600_000;
	/** Prometheus' history, then the samples taken since. */
	const joined = (history: Sample[], live: Sample[]) => {
		const last = history[history.length - 1]?.at ?? 0;
		return [...history, ...live.filter((s) => s.at > last)];
	};
	const cpuTrend = $derived(stored?.cpu.length ? joined(stored.cpu, cpuHistory) : cpuHistory);
	const memoryTrend = $derived(stored?.memory.length ? joined(stored.memory, memoryHistory) : memoryHistory);
	const trendWindow = $derived(stored?.cpu.length ? HOUR : undefined);

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
		const loadHistory = async () => {
			if (!app.has('prometheus')) return;
			try {
				const history = await backend.usageHistory(ns, HOUR / 1000);
				if (cancelled) return;
				const samples = (id: string) =>
					(history.charts.find((c) => c.id === id)?.series[0]?.points ?? []).map(([at, value]) => ({ at: at * 1000, value }));
				stored = { cpu: samples('cpu'), memory: samples('memory') };
			} catch {
				stored = null;
			}
		};
		stored = null;
		load();
		loadHistory();
		const refresh = setInterval(() => document.hidden || load(), REFRESH_MS);
		const refreshHistory = setInterval(() => document.hidden || loadHistory(), 60_000);
		const clock = setInterval(() => (now = Date.now()), 1000);
		return () => {
			cancelled = true;
			clearInterval(refresh);
			clearInterval(refreshHistory);
			clearInterval(clock);
		};
	});

	function openList(ref: ResourceRef) {
		const resource = findResource(app.resources, ref);
		if (resource) app.open(resource);
		else app.toast(`${ref.plural} is not served by this cluster`, 'bad');
	}

	const pods = $derived(pulse?.kinds.find((k) => k.kind === 'Pod'));
	const nodes = $derived(pulse?.kinds.find((k) => k.kind === 'Node'));
	const usageList: ResourceRef = $derived(
		namespace ? { group: '', version: 'v1', plural: 'pods' } : { group: '', version: 'v1', plural: 'nodes' }
	);

	const kindGroups = $derived(splitKinds(pulse?.kinds ?? []));
	let showHealthy = $state(false);
	const shownKinds = $derived(showHealthy ? [...kindGroups.problems, ...kindGroups.healthy] : kindGroups.problems);

	/** What arrow keys move between and Enter opens: the cards on top, then the kinds. */
	const tiles = $derived<(() => void)[]>([
		() => openList({ group: '', version: 'v1', plural: 'pods' }),
		() => openList({ group: '', version: 'v1', plural: 'nodes' }),
		() => openList(usageList),
		() => openList(usageList),
		...shownKinds.map((k) => () => openList(k.resource))
	]);
	const KIND_TILES = 4;

	const attention = $derived(attentionItems(pulse?.events.recent ?? []));
	const scopeText = $derived(namespace ? `Namespace ${namespace}` : 'All namespaces');

	function openCertificate(alert: CertificateAlert) {
		const resource = findResource(app.resources, alert.object.resource);
		if (resource) app.showDetails(resource, { ...alert.object, context: app.cluster?.context });
	}

	function openEvent(event: RecentEvent) {
		const resource = findResource(app.resources, event.object.resource);
		if (resource) app.showDetails(resource, { ...event.object, resource });
		else app.toast(`${event.objectKind} ${event.object.name} cannot be opened here`, 'bad');
	}

	function cores(millicores: number) {
		const value = millicores / 1000;
		return value < 10 ? value.toFixed(1) : Math.round(value).toString();
	}

	const percent = (value: number) => `${Math.round(value * 100)}%`;

	const KIND_ICONS: Record<string, IconName> = {
		Pod: 'pod',
		Deployment: 'deployment',
		StatefulSet: 'statefulset',
		DaemonSet: 'daemonset',
		Job: 'job',
		CronJob: 'cronjob',
		Node: 'node',
		PersistentVolumeClaim: 'volume',
		PersistentVolume: 'volume',
		Certificate: 'lock'
	};

	function activate(index: number) {
		cursor = index;
		tiles[index]?.();
	}

	function move(delta: number) {
		keyboard = true;
		cursor = Math.max(0, Math.min(tiles.length - 1, cursor + delta));
		tick().then(() => body?.querySelector<HTMLElement>(`[data-tile="${cursor}"]`)?.focus());
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
					tiles[cursor]?.();
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
		<nav class="crumbs" aria-label="Breadcrumb">
			<span>{app.cluster?.context}</span>
			<Icon name="next" size={12} />
			<span>Overview</span>
		</nav>
		<div class="heading">
			<div>
				<h1>Cluster overview</h1>
				<p class="subtitle">{scopeText}{app.environment ? ` · ${app.environment}` : ''}</p>
			</div>
			<div class="state">
				{#if attention.length}
					<span class="tag warn"><Icon name="warning" size={14} />{attention.length} need attention</span>
				{/if}
				{#if updated}
					<span class="live" title="Refreshes every {REFRESH_MS / 1000} seconds">
						<span class="dot"></span>Updated {age(new Date(updated).toISOString(), now) || '0s'} ago
					</span>
				{/if}
			</div>
		</div>
	</header>

	{#if error}
		<div class="error" role="alert">
			Could not load the overview: {error}{pulse ? ' Showing the last values.' : ''}
		</div>
	{/if}

	{#if pulse}
		<div class="body" bind:this={body}>
			<div class="kpis">
				{@render countCard(0, 'Healthy pods', 'pod', pods, namespace ? `In ${namespace}` : 'All namespaces')}
				{@render countCard(1, 'Ready nodes', 'node', nodes, 'Cluster-wide')}
				{@render usageCard(2, 'CPU utilization', 'benchmark', pulse.cpu, (m) => `${cores(m)} cores`)}
				{@render usageCard(3, 'Memory utilization', 'volume', pulse.memory, memory)}
			</div>

			<div class="columns">
				<div class="main-column">
					<section class="card" aria-labelledby="attention-title">
						<div class="card-head">
							<div>
								<h2 id="attention-title">Needs attention <span class="n">{attention.length}</span></h2>
								<p>Warnings of the last hour, one entry per object.</p>
							</div>
							<button class="link" onclick={() => openList({ group: '', version: 'v1', plural: 'events' })}>
								All events <Icon name="arrow" size={15} />
							</button>
						</div>
						{#if pulse.events.error}
							<p class="empty">Warnings are not available: {pulse.events.error}</p>
						{:else if attention.length === 0}
							<p class="empty"><Icon name="ok" size={16} /> Nothing needs a look in {scopeText.toLowerCase()}.</p>
						{:else}
							{#each attention as item (`${item.event.object.namespace}/${item.event.object.name}/${item.event.objectKind}`)}
								<button class="signal" onclick={() => openEvent(item.event)}>
									<span class="mark {item.severity}"
										><Icon name={item.severity === 'critical' ? 'critical' : 'warning'} /></span
									>
									<span class="copy">
										<span class="headline">
											<strong>{item.reasons.join(', ')}</strong>
											<span class="tag {item.severity === 'critical' ? 'bad' : 'warn'}">
												{item.severity === 'critical' ? 'Critical' : 'Warning'}
											</span>
										</span>
										<span class="message" title={item.event.message}
											>{shortMessage(item.event.reason, item.event.message, item.count)}</span
										>
										<span class="where">
											<code
												>{item.event.objectKind}
												{item.event.object.namespace ? `${item.event.object.namespace} / ` : ''}{item.event.object
													.name}</code
											>
											<span>{item.count}× · {age(item.event.lastSeen, app.snapshot?.taken ?? now) || 'just now'} ago</span>
										</span>
									</span>
									<span class="go">Investigate <Icon name="arrow" size={15} /></span>
								</button>
							{/each}
						{/if}
					</section>

					<section class="card" aria-labelledby="events-title">
						<div class="card-head">
							<h2 id="events-title">Recent warnings</h2>
							<span class="muted">{pulse.events.warning} warnings · {pulse.events.normal} normal, last hour</span>
						</div>
						{#if pulse.events.recent.length}
							<ul class="events">
								{#each pulse.events.recent.slice(0, RECENT) as event, i (i)}
									<li>
										<button onclick={() => openEvent(event)} title="Open {event.object.name}">
											<span class="when">{age(event.lastSeen, now) || 'now'}</span>
											<span class="tag warn">{event.reason}</span>
											<span class="what">
												<span class="message">{event.message}</span>
												<span class="muted"
													>{event.objectKind} / {event.object.name}{event.count > 1 ? ` · ${event.count}×` : ''}</span
												>
											</span>
										</button>
									</li>
								{/each}
							</ul>
						{:else}
							<p class="empty">No warnings in the last hour.</p>
						{/if}
					</section>
				</div>

				<div class="side-column">
					<section class="card" aria-labelledby="health-title">
						<div class="card-head">
							<h2 id="health-title">Resource health</h2>
						</div>
						<ul class="kinds">
							{#each shownKinds as kind, i (kind.kind)}
								{@render kindRow(kind, i + KIND_TILES)}
							{/each}
							{#if kindGroups.healthy.length}
								<li>
									<button class="fold" aria-expanded={showHealthy} onclick={() => (showHealthy = !showHealthy)}>
										<span class="kind-name">
											<Icon name="ok" />
											{kindGroups.healthy.length}
											{kindGroups.healthy.length === 1 ? 'kind is' : 'kinds are'} healthy
										</span>
										<span class="muted">{showHealthy ? 'Hide' : 'Show'}</span>
									</button>
								</li>
							{/if}
						</ul>
					</section>

					{#if pulse.certificates.length}
						<section class="card" aria-labelledby="certificates-title">
							<div class="card-head">
								<h2 id="certificates-title">Certificates <span class="n">{pulse.certificates.length}</span></h2>
								<button class="link" onclick={() => openList(pulse!.certificates[0].object.resource)}>
									All <Icon name="arrow" size={15} />
								</button>
							</div>
							<ul class="events">
								{#each pulse.certificates as alert (`${alert.object.namespace}/${alert.object.name}`)}
									<li>
										<button onclick={() => openCertificate(alert)} title="Open {alert.object.name}">
											<span class="when" title={alert.notAfter ?? undefined}>{alert.notAfter ? relative(alert.notAfter, now) : '—'}</span>
											<span class="tag {alert.health === 'failed' ? 'bad' : 'warn'}">{alert.health === 'failed' ? 'Critical' : 'Warning'}</span>
											<span class="what">
												<span class="message">{alert.detail}</span>
												<span class="muted">{alert.object.namespace ? `${alert.object.namespace} / ` : ''}{alert.object.name}</span>
											</span>
										</button>
									</li>
								{/each}
							</ul>
						</section>
					{/if}

					<section class="card" aria-labelledby="trend-title">
						<div class="card-head">
							<h2 id="trend-title">Usage trend</h2>
							<span class="muted">{namespace ? 'This namespace' : 'Cluster-wide'}</span>
						</div>
						<div class="trends">
							{@render trend('CPU', pulse.cpu, cpuTrend, (m) => `${cores(m)} cores`)}
							{@render trend('Memory', pulse.memory, memoryTrend, memory)}
						</div>
						<p class="note">
							{trendWindow ? 'The last hour from Prometheus, measured use.' : 'Kept for this session; Kubri stores no history.'}
						</p>
					</section>
				</div>
			</div>
		</div>
	{:else if !error}
		<div class="loading">Loading the overview…</div>
	{/if}
</section>

{#snippet countCard(index: number, label: string, icon: IconName, kind: KindPulse | undefined, scope: string)}
	<button class="kpi" class:cursor={keyboard && cursor === index} data-tile={index} onclick={() => activate(index)}>
		<span class="kpi-label">{label}<Icon name={icon} /></span>
		{#if !kind}
			<span class="denied">Not shown</span>
			<span class="kpi-note">{label === 'Ready nodes' ? 'Nodes show for all namespaces' : 'Not listed'}</span>
		{:else if kind.error}
			<span class="denied" title={kind.error}>No access</span>
			<span class="kpi-note">{scope}</span>
		{:else}
			<span class="kpi-value">{kind.ok}<small> / {kind.total}</small></span>
			<span class="kpi-note">
				{kind.warning || kind.failed
					? [kind.warning && `${kind.warning} warning`, kind.failed && `${kind.failed} failing`]
							.filter(Boolean)
							.join(' · ')
					: kind.total
						? `${scope} · all healthy`
						: `${scope} · none`}
			</span>
			<span class="bar" aria-hidden="true">
				{#each [['ok', kind.ok], ['warning', kind.warning], ['failed', kind.failed]] as [health, count] (health)}
					{#if Number(count) > 0}<span class="segment {health}" style:flex-grow={count}></span>{/if}
				{/each}
			</span>
		{/if}
	</button>
{/snippet}

{#snippet usageCard(index: number, label: string, icon: IconName, u: Utilization, format: (v: number) => string)}
	{@const level = fill(u)}
	<button
		class="kpi"
		class:cursor={keyboard && cursor === index}
		data-tile={index}
		onclick={() => activate(index)}
		title="Open {namespace ? 'pods' : 'nodes'}"
	>
		<span class="kpi-label">{label}<Icon name={icon} /></span>
		{#if level && u.capacity}
			<span class="kpi-value">{Math.round(level.value * 100)}<small>%{level.measured ? '' : ' requested'}</small></span>
			<span class="kpi-note">
				{#if level.measured && u.used !== null}
					{format(u.used)} of {format(u.capacity)}{u.requested !== null ? ` · ${format(u.requested)} requested` : ''}
				{:else}
					{format(u.requested ?? 0)} of {format(u.capacity)} · no metrics-server
				{/if}
			</span>
			<span
				class="meter"
				role="meter"
				aria-valuemin="0"
				aria-valuemax="100"
				aria-valuenow={Math.round(level.value * 100)}
				aria-label="{label} {level.measured ? 'used' : 'requested'}"
			>
				<span
					class="fill {meterTone(level.value) === 'accent' ? '' : meterTone(level.value)}"
					style:width={percent(Math.min(level.value, 1))}
				></span>
				{#if level.measured && u.requested !== null}
					<span class="request" style:left={percent(Math.min(u.requested / u.capacity, 1))} title="Requested"></span>
				{/if}
			</span>
		{:else}
			<span class="denied">Not available</span>
			<span class="kpi-note">Needs access to nodes{u.used === null ? ' and metrics-server' : ''}</span>
		{/if}
	</button>
{/snippet}

{#snippet kindRow(kind: KindPulse, index: number)}
	<li>
		<button class:cursor={keyboard && cursor === index} data-tile={index} onclick={() => activate(index)}>
			<span class="kind-name"><Icon name={KIND_ICONS[kind.kind] ?? 'resource'} />{kindLabel(kind)}</span>
			{#if kind.error}
				<span class="muted" title={kind.error}>No access</span>
			{:else if kind.total === 0}
				<span class="muted">None</span>
			{:else}
				<span class="kind-counts">
					{#if kind.failed}<span class="bad">{kind.failed} failing</span>{/if}
					{#if kind.warning}<span class="warn">{kind.warning} warning</span>{/if}
					<span><strong>{kind.ok}</strong> / {kind.total} healthy</span>
				</span>
				<span class="bar" aria-hidden="true">
					{#each [['ok', kind.ok], ['warning', kind.warning], ['failed', kind.failed]] as [health, count] (health)}
						{#if Number(count) > 0}<span class="segment {health}" style:flex-grow={count}></span>{/if}
					{/each}
				</span>
			{/if}
		</button>
	</li>
{/snippet}

{#snippet trend(label: string, u: Utilization, history: Sample[], format: (v: number) => string)}
	{@const level = fill(u)}
	<div class="trend">
		<span class="trend-label">
			{label}
			{#if level && u.capacity}
				<strong>{format(level.measured ? (u.used ?? 0) : (u.requested ?? 0))}</strong>
				<span class="muted">/ {format(u.capacity)}</span>
			{/if}
		</span>
		{#if level}
			<Sparkline samples={history} {format} label="{label} {level.measured ? 'used' : 'requested'}" {now} window={trendWindow} />
		{:else}
			<span class="muted">Not available</span>
		{/if}
	</div>
{/snippet}

<style>
	section {
		display: flex;
		flex-direction: column;
		min-height: 0;
		min-width: 0;
	}
	header {
		padding: 20px 24px 16px;
	}
	.crumbs {
		display: flex;
		align-items: center;
		gap: 6px;
		margin-bottom: 6px;
		font-size: 12px;
		color: var(--text-muted);
	}
	.heading {
		display: flex;
		flex-wrap: wrap;
		align-items: flex-end;
		justify-content: space-between;
		gap: 10px 16px;
	}
	h1 {
		margin: 0;
		font-size: 26px;
		letter-spacing: -0.03em;
	}
	.subtitle {
		margin: 4px 0 0;
		font-size: 13px;
		color: var(--text-muted);
	}
	.state {
		display: flex;
		align-items: center;
		gap: 12px;
	}
	.live {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		font-size: 12px;
		color: var(--ok);
	}
	.dot {
		width: 7px;
		height: 7px;
		border-radius: 50%;
		background: currentColor;
	}
	.tag {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		padding: 2px 7px;
		border-radius: 5px;
		background: var(--surface-2);
		font-size: 11px;
		font-weight: 600;
		white-space: nowrap;
	}
	.tag.warn {
		background: color-mix(in srgb, var(--warn) 15%, var(--surface-1));
		color: var(--warn);
	}
	.tag.bad {
		background: color-mix(in srgb, var(--bad) 13%, var(--surface-1));
		color: var(--bad);
	}
	.error {
		margin: 0 24px 12px;
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
		padding: 0 24px 24px;
	}
	.kpis {
		display: grid;
		grid-template-columns: repeat(4, minmax(0, 1fr));
		border: 1px solid var(--border);
		border-radius: 10px;
		background: var(--surface-1);
		overflow: hidden;
	}
	.kpi {
		display: flex;
		flex-direction: column;
		gap: 6px;
		min-width: 0;
		padding: 18px 20px;
		border: none;
		border-left: 1px solid var(--border);
		background: none;
		text-align: left;
	}
	.kpi:first-child {
		border-left: none;
	}
	.kpi:hover {
		background: color-mix(in srgb, var(--surface-2) 50%, transparent);
	}
	.kpi.cursor,
	.kpi:focus-visible,
	.kinds button.cursor,
	.kinds button:focus-visible {
		outline: 2px solid var(--accent);
		outline-offset: -2px;
	}
	.kpi-label {
		display: flex;
		justify-content: space-between;
		font-size: 12.5px;
		color: var(--text-muted);
	}
	.kpi-value {
		font-size: 30px;
		font-weight: 600;
		line-height: 1.1;
		letter-spacing: -0.02em;
	}
	.kpi-value small {
		font-size: 17px;
		font-weight: 400;
		color: var(--text-muted);
	}
	.kpi-note {
		min-height: 1.4em;
		font-size: 11.5px;
		color: var(--text-muted);
		overflow-wrap: anywhere;
	}
	.denied {
		font-size: 17px;
		font-weight: 600;
		color: var(--text-muted);
	}
	.meter {
		position: relative;
		height: 5px;
		margin-top: 6px;
		border-radius: 3px;
		background: var(--surface-2);
	}
	.fill {
		position: absolute;
		inset: 0 auto 0 0;
		border-radius: 3px;
		background: var(--accent);
	}
	.fill.warn {
		background: var(--warn-mark);
	}
	.fill.bad {
		background: var(--bad);
	}
	.request {
		position: absolute;
		top: -3px;
		bottom: -3px;
		width: 2px;
		margin-left: -1px;
		background: var(--text);
	}
	.columns {
		display: grid;
		grid-template-columns: minmax(0, 1.75fr) minmax(280px, 1fr);
		gap: 20px;
		margin-top: 20px;
		align-items: start;
	}
	.main-column,
	.side-column {
		display: flex;
		flex-direction: column;
		gap: 20px;
		min-width: 0;
	}
	.card {
		border: 1px solid var(--border);
		border-radius: 10px;
		background: var(--surface-1);
		overflow: hidden;
	}
	.card-head {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: 12px;
		padding: 16px 20px;
	}
	.card-head p {
		margin: 4px 0 0;
		font-size: 12px;
		color: var(--text-muted);
	}
	h2 {
		display: flex;
		align-items: center;
		gap: 8px;
		margin: 0;
		font-size: 16px;
		letter-spacing: -0.01em;
	}
	.n {
		padding: 0 6px;
		border-radius: 4px;
		background: var(--surface-2);
		font-size: 11px;
		font-weight: 600;
	}
	.link {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		flex: none;
		padding: 4px 0;
		border: none;
		background: none;
		color: var(--accent-strong);
		font-size: 12.5px;
		font-weight: 600;
	}
	.empty {
		display: flex;
		align-items: center;
		gap: 8px;
		margin: 0;
		padding: 0 20px 18px;
		font-size: 12.5px;
		color: var(--text-muted);
	}
	.signal {
		display: flex;
		align-items: flex-start;
		gap: 14px;
		width: 100%;
		padding: 16px 20px;
		border: none;
		border-top: 1px solid var(--border);
		background: none;
		text-align: left;
	}
	.signal:hover {
		background: color-mix(in srgb, var(--surface-2) 50%, transparent);
	}
	.mark {
		display: grid;
		place-items: center;
		width: 30px;
		height: 30px;
		flex: none;
		border-radius: 7px;
	}
	.mark.critical {
		background: color-mix(in srgb, var(--bad) 12%, var(--surface-1));
		color: var(--bad);
	}
	.mark.warning {
		background: color-mix(in srgb, var(--warn) 15%, var(--surface-1));
		color: var(--warn);
	}
	.copy {
		display: grid;
		gap: 5px;
		flex: 1;
		min-width: 0;
	}
	.headline {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 8px;
	}
	.message {
		font-size: 12.5px;
		color: var(--text-muted);
		overflow-wrap: anywhere;
	}
	.where {
		display: flex;
		flex-wrap: wrap;
		gap: 4px 12px;
		font-size: 11.5px;
		color: var(--text-muted);
	}
	.where code {
		font-family: var(--mono);
		color: var(--text);
		overflow-wrap: anywhere;
	}
	.go {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		align-self: center;
		color: var(--accent-strong);
		font-size: 12px;
		font-weight: 600;
		white-space: nowrap;
	}
	.events,
	.kinds {
		margin: 0;
		padding: 0;
		list-style: none;
	}
	.events button {
		display: grid;
		grid-template-columns: 52px auto minmax(0, 1fr);
		align-items: start;
		gap: 12px;
		width: 100%;
		padding: 11px 20px;
		border: none;
		border-top: 1px solid var(--border-subtle);
		background: none;
		text-align: left;
	}
	.events button:hover {
		background: color-mix(in srgb, var(--surface-2) 50%, transparent);
	}
	.when {
		font-family: var(--mono);
		font-size: 11.5px;
		color: var(--text-muted);
	}
	.what {
		display: grid;
		gap: 2px;
		min-width: 0;
		font-size: 12px;
	}
	.what .message {
		color: var(--text);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.kinds button {
		display: grid;
		grid-template-columns: minmax(0, 1fr) auto;
		align-items: center;
		gap: 8px 12px;
		width: 100%;
		padding: 11px 20px;
		border: none;
		border-top: 1px solid var(--border-subtle);
		background: none;
		font-size: 13px;
		text-align: left;
	}
	.kinds button:hover {
		background: color-mix(in srgb, var(--surface-2) 50%, transparent);
	}
	.kind-name {
		display: flex;
		align-items: center;
		gap: 10px;
		min-width: 0;
	}
	.kind-counts {
		display: flex;
		flex-wrap: wrap;
		justify-content: flex-end;
		gap: 2px 10px;
		min-width: 0;
		font-size: 12px;
		color: var(--text-muted);
	}
	.kind-counts > span {
		white-space: nowrap;
	}
	.kind-name {
		overflow-wrap: anywhere;
	}
	.kinds .fold {
		color: var(--text-muted);
	}
	.kind-counts strong {
		color: var(--text);
	}
	.bar {
		grid-column: 1 / -1;
		display: flex;
		gap: 2px;
		height: 5px;
		border-radius: 3px;
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
	.trends {
		display: grid;
		gap: 14px;
		padding: 0 20px;
	}
	.trend {
		display: grid;
		gap: 4px;
	}
	.trend-label {
		display: flex;
		align-items: baseline;
		gap: 6px;
		font-size: 12px;
		color: var(--text-muted);
	}
	.trend-label strong {
		margin-left: auto;
		color: var(--text);
		font-size: 14px;
	}
	.note {
		margin: 12px 20px 16px;
		font-size: 11.5px;
		color: var(--text-muted);
	}
	.muted {
		color: var(--text-muted);
		font-size: 12px;
	}
	.bad {
		color: var(--bad);
	}
	.warn {
		color: var(--warn);
	}

	@media (max-width: 1100px) {
		.kpis {
			grid-template-columns: repeat(2, minmax(0, 1fr));
		}
		.kpi:nth-child(3) {
			border-left: none;
		}
		.kpi:nth-child(n + 3) {
			border-top: 1px solid var(--border);
		}
		.columns {
			grid-template-columns: minmax(0, 1fr);
		}
	}
	@media (max-width: 720px) {
		header {
			padding: 16px 12px 12px;
		}
		.body {
			padding: 0 12px 16px;
		}
		.go {
			display: none;
		}
		.kpi {
			padding: 14px;
		}
		.kpi-value {
			font-size: 24px;
		}
	}
</style>
