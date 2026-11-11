<script module lang="ts">
	import type { Sample } from '../pulse';

	/** Trend lines survive leaving the page, per context and namespace. */
	const kept = new Map<string, { cpu: Sample[]; memory: Sample[] }>();
</script>

<script lang="ts">
	import { forControl, reachedByKeys } from '../keys';
	import { tick } from 'svelte';
	import { backend, type CertificateAlert, type KindPulse, type Pulse, type RecentEvent, type ResourceRef, type Utilization, type WaitingPod } from '../api';
	import { age, cpuText, errorMessage, memoryText as memory, relative } from '../format';
	import { fleetState, serverHost, workloads } from '../fleet';
	import { attentionTitle } from '../causes';
	import { attentionItems, fill, kindLabel, meterTone, record, restartingContainer, room, shortMessage, splitKinds, type AttentionItem } from '../pulse';
	import { lastLogLine } from '../lastlog';
	import { app, findResource } from '../state.svelte';
	import { plural, t } from '../i18n';
	import Crumbs from './Crumbs.svelte';
	import { nearestVertically } from '../roving';
	import { every } from '../poll';
	import HealthIcon from './HealthIcon.svelte';
	import OtherContexts from './OtherContexts.svelte';
	import Icon, { type IconName } from './Icon.svelte';
	import Sparkline from './Sparkline.svelte';

	const REFRESH_MS = 30_000;
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
	/** Whether the focus is on something other than a tile, such as a link reached with Tab. */
	let away = $state(false);
	/** The one tile marked for the keys: the arrows and Tab move the same mark. */
	const marked = (index: number) => keyboard && !away && cursor === index;
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
			if (!app.integrations.has('prometheus')) return;
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
		const stopRefresh = every(REFRESH_MS, load);
		const stopHistory = every(60_000, loadHistory);
		const clock = setInterval(() => (now = Date.now()), 1000);
		return () => {
			cancelled = true;
			stopRefresh();
			stopHistory();
			clearInterval(clock);
		};
	});

	function openList(ref: ResourceRef) {
		const resource = findResource(app.resources, ref);
		if (resource) app.open(resource);
		else app.toast(t('{plural} is not served by this cluster', { plural: ref.plural }), 'bad');
	}

	const pods = $derived(pulse?.kinds.find((k) => k.kind === 'Pod'));
	const nodes = $derived(pulse?.kinds.find((k) => k.kind === 'Node'));
	const work = $derived(pulse ? workloads(pulse.kinds) : null);
	/** The same line the cluster's card on the Clusters page shows. */
	const health = $derived(
		pulse && app.cluster ? fleetState({ ...app.cluster, error: null, millis: 0, kinds: pulse.kinds, cpu: null, memory: null }) : null
	);
	const usageList: ResourceRef = $derived(
		namespace ? { group: '', version: 'v1', plural: 'pods' } : { group: '', version: 'v1', plural: 'nodes' }
	);

	const kindGroups = $derived(splitKinds(pulse?.kinds ?? []));
	let showHealthy = $state(false);
	let showRecent = $state(false);
	const shownKinds = $derived(showHealthy ? [...kindGroups.problems, ...kindGroups.healthy] : kindGroups.problems);

	/** What arrow keys move between and Enter opens: the cards on top, then the kinds. */
	const tiles = $derived<(() => void)[]>([
		() => openList({ group: '', version: 'v1', plural: 'pods' }),
		() => openList({ group: '', version: 'v1', plural: 'nodes' }),
		() => openList(work?.resource ?? { group: 'apps', version: 'v1', plural: 'deployments' }),
		() => openList(usageList),
		() => openList(usageList),
		...shownKinds.map((k) => () => openList(k.resource))
	]);
	const KIND_TILES = 5;

	const attention = $derived(attentionItems(pulse?.events.recent ?? []));

	/** The last words of containers that keep restarting, by pod and container. */
	let lastLines = $state<Record<string, string | null>>({});
	const asked = new Set<string>();
	const logKey = (item: AttentionItem) =>
		`${app.cluster?.context}/${item.event.object.namespace}/${item.event.object.name}/${restartingContainer(item.event.message)}`;
	$effect(() => {
		for (const item of attention) {
			const container = restartingContainer(item.event.message);
			const key = logKey(item);
			if (!container || item.event.objectKind !== 'Pod' || asked.has(key)) continue;
			asked.add(key);
			lastLogLine({ ...item.event.object, context: app.cluster?.context }, container).then((line) => {
				if (line) lastLines[key] = line;
			});
		}
	});
	const scopeText = $derived(namespace ? t('Namespace {name}', { name: namespace }) : t('All namespaces'));
	const clusterText = $derived(
		app.cluster ? [serverHost(app.cluster.server), app.cluster.version].filter(Boolean).join(' · ') : ''
	);

	function openCertificate(alert: CertificateAlert) {
		const resource = findResource(app.resources, alert.object.resource);
		if (resource) app.showDetails(resource, { ...alert.object, context: app.cluster?.context });
	}

	function openEvent(event: RecentEvent) {
		const resource = findResource(app.resources, event.object.resource);
		if (resource) app.showDetails(resource, { ...event.object, resource });
		else app.toast(t('{kind} {name} cannot be opened here', { kind: event.objectKind, name: event.object.name }), 'bad');
	}

	/** CPU in cores, or in millicores for those who chose kubectl's units. */
	function cores(millicores: number) {
		return cpuText(millicores, true);
	}

	const percent = (value: number) => `${Math.round(value * 100)}%`;

	/** The waiting pods, one per line, for the tooltip. */
	function waitingTitle(waiting: WaitingPod[], resource: 'cpu' | 'memory', format: (v: number) => string) {
		const lines = waiting.map((w) => `${w.object.namespace ? `${w.object.namespace}/` : ''}${w.object.name}: ${format(w[resource])}`);
		return [t('Waiting for a node:'), ...lines].join('\n');
	}

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

	/** Down or up to the nearest tile in the next row, as the tiles sit on the page. */
	function moveVertically(down: boolean) {
		const boxes = tiles.map((_, i) => {
			const r = body?.querySelector<HTMLElement>(`[data-tile="${i}"]`)?.getBoundingClientRect();
			return r ? { left: r.left, top: r.top, width: r.width, height: r.height } : { left: 0, top: 0, width: 0, height: 0 };
		});
		// The first key shows where the cursor is; at an edge it stays.
		const next = marked(cursor) ? nearestVertically(boxes, cursor, down) : null;
		move(next === null ? 0 : next - cursor);
	}

	/** Tab moves the cursor too: to the tile it reaches, or off the tiles while it is elsewhere. */
	function onfocusin(e: FocusEvent) {
		const tile = (e.target as HTMLElement).closest?.<HTMLElement>('[data-tile]');
		if (tile && body?.contains(tile)) {
			cursor = Number(tile.dataset.tile);
			if (reachedByKeys(tile)) keyboard = true;
			away = false;
		} else away = e.target !== document.body;
	}

	function onfocusout(e: FocusEvent) {
		// Nothing has the focus now, so the arrows start from the cursor again.
		if (!e.relatedTarget) away = false;
	}

	function onkeydown(e: KeyboardEvent) {
		if (app.overlayOpen || e.ctrlKey || e.metaKey || e.altKey) return;
		if (forControl(e)) return;
		const handled = (() => {
			switch (e.key) {
				case 'ArrowRight':
				case 'l':
					move(1);
					return true;
				case 'ArrowLeft':
				case 'h':
					move(-1);
					return true;
				case 'ArrowDown':
				case 'j':
					moveVertically(true);
					return true;
				case 'ArrowUp':
				case 'k':
					moveVertically(false);
					return true;
				case 'Enter':
					tiles[cursor]?.();
					return true;
				case 'Escape':
					if (!app.details) return false;
					app.details = null;
					app.layout.maximized = false;
					return true;
				case 'f':
					if (!app.details) return false;
					app.layout.maximized = !app.layout.maximized;
					return true;
			}
			return false;
		})();
		if (handled) e.preventDefault();
	}
</script>

<svelte:window {onkeydown} {onfocusin} {onfocusout} />

<section>
	<header>
		<Crumbs crumbs={[t('Overview')]} />
		<div class="heading">
			<div>
				<h1>{t('Overview')}</h1>
				<p class="subtitle">
					{scopeText}{app.environment ? ` · ${app.environment}` : ''}
					{#if clusterText}<span class="mono">· {clusterText}</span>{/if}
				</p>
			</div>
			<div class="state">
				<!-- One line: the cards below count pods and workloads, so it says what needs a look. -->
				{#if attention.length}
					<span class="tag warn" role="status"><Icon name="warning" size={14} />{plural(attention.length, '{n} needs attention', '{n} need attention')}</span>
				{:else if health?.health}
					<span class="health" role="status"><HealthIcon health={health.health} size={11} />{health.summary}</span>
				{/if}
				{#if updated}
					<span class="live" title={t('Refreshes every {seconds} seconds', { seconds: REFRESH_MS / 1000 })}>
						<span class="dot"></span>{t('Updated {age} ago', { age: age(new Date(updated).toISOString(), now) || '0s' })}
					</span>
				{/if}
			</div>
		</div>
	</header>

	{#if error}
		<div class="error" role="alert">
			{t('Could not load the overview: {error}', { error })}{pulse ? ` ${t('Showing the last values.')}` : ''}
		</div>
	{/if}

	{#if pulse}
		<div class="body" bind:this={body}>
			<div class="kpis">
				{@render countCard(0, t('Healthy pods'), 'pod', pods, namespace ? t('In {namespace}', { namespace }) : t('All namespaces'))}
				{@render countCard(1, t('Ready nodes'), 'node', nodes, t('Cluster-wide'))}
				{@render countCard(2, t('Ready workloads'), 'deployment', work ?? undefined, namespace ? t('In {namespace}', { namespace }) : t('All namespaces'))}
				{@render usageCard(3, t('CPU'), 'benchmark', pulse.cpu, cores, 'cpu')}
				{@render usageCard(4, t('Memory'), 'volume', pulse.memory, memory, 'memory')}
			</div>

			<div class="columns">
				<div class="main-column">
					<section class="card" aria-labelledby="attention-title">
						<div class="card-head">
							<div>
								<h2 id="attention-title">{t('Needs attention')} <span class="n">{attention.length}</span></h2>
								<p>{t('Warnings of the last hour, one entry per workload.')}</p>
							</div>
							<button class="link" onclick={() => openList({ group: '', version: 'v1', plural: 'events' })}>
								{t('All events')} <Icon name="arrow" size={15} />
							</button>
						</div>
						{#if pulse.events.error}
							<p class="empty">{t('Warnings are not available: {error}', { error: pulse.events.error })}</p>
						{:else if attention.length === 0}
							<p class="empty"><Icon name="ok" size={16} /> {namespace ? t('Nothing needs a look in namespace {name}.', { name: namespace }) : t('Nothing needs a look in all namespaces.')}</p>
						{:else}
							{#each attention as item (`${item.kind}/${item.namespace}/${item.name}`)}
								{@const when = age(item.event.lastSeen, app.snapshot?.taken ?? now)}
								{@const pods = item.objects.filter((o) => o.resource.plural === 'pods')}
								{@const line = lastLines[logKey(item)]}
								<button class="signal" onclick={() => openEvent(item.event)}>
									<span class="mark {item.severity}"
										><Icon name={item.severity === 'critical' ? 'critical' : 'warning'} /></span
									>
									<span class="copy">
										<span class="headline">
											<strong>{attentionTitle(item.name, item.reasons, item.event.message)}</strong>
											<span class="tag {item.severity === 'critical' ? 'bad' : 'warn'}">
												{item.severity === 'critical' ? t('Critical') : t('Warning')}
											</span>
										</span>
										<span class="message" title="{item.reasons.join(', ')}: {item.event.message}"
											>{shortMessage(item.event.reason, item.event.message)}</span
										>
										{#if line}
											<span class="last-log" title={t('From the log of the run that ended')}
												>{t('Last log line:')} <code>{line}</code></span
											>
										{/if}
										<span class="where">
											<span><code>{item.kind}</code>{item.namespace ? ` · ${item.namespace}` : ''}</span>
											{#if pods.length && item.kind !== 'Pod'}
												<span title={pods.map((o) => o.name).join('\n')}
													>{plural(pods.length, '{n} pod affected', '{n} pods affected')}</span
												>
											{/if}
											<span title={t('{n} warnings since they began', { n: item.count })}
												>{when ? t('{age} ago', { age: when }) : t('just now')}</span
											>
										</span>
									</span>
									<span class="go">{t('Investigate')} <Icon name="arrow" size={15} /></span>
								</button>
							{/each}
						{/if}
					</section>

					<section class="card" aria-labelledby="events-title">
						<div class="card-head">
							<div>
								<h2 id="events-title">{t('Recent warnings')}</h2>
								<p>{t('{warnings} warnings · {normal} normal, last hour', { warnings: pulse.events.warning, normal: pulse.events.normal })}</p>
							</div>
							{#if pulse.events.recent.length}
								<!-- The same warnings as above, as Kubernetes words them: folded, for those who want them. -->
								<button class="link" aria-expanded={showRecent} onclick={() => (showRecent = !showRecent)}>
									{showRecent ? t('Hide') : t('Show')}
								</button>
							{/if}
						</div>
						{#if pulse.events.recent.length && !showRecent}
							<!-- Folded. -->
						{:else if pulse.events.recent.length}
							<ul class="events">
								{#each pulse.events.recent.slice(0, RECENT) as event, i (i)}
									<li>
										<button onclick={() => openEvent(event)} title={t('Open {name}', { name: event.object.name })}>
											<span class="when">{age(event.lastSeen, now) || t('now')}</span>
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
							<p class="empty">{t('No warnings in the last hour.')}</p>
						{/if}
					</section>
				</div>

				<div class="side-column">
					<section class="card" aria-labelledby="health-title">
						<div class="card-head">
							<h2 id="health-title">{t('Resource health')}</h2>
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
											{plural(kindGroups.healthy.length, '{n} kind is healthy', '{n} kinds are healthy')}
										</span>
										<span class="muted">{showHealthy ? t('Hide') : t('Show')}</span>
									</button>
								</li>
							{/if}
						</ul>
					</section>

					{#if pulse.certificates.length}
						<section class="card" aria-labelledby="certificates-title">
							<div class="card-head">
								<h2 id="certificates-title">{t('Certificates')} <span class="n">{pulse.certificates.length}</span></h2>
								<button class="link" onclick={() => openList(pulse!.certificates[0].object.resource)}>
									{t('All')} <Icon name="arrow" size={15} />
								</button>
							</div>
							<ul class="events">
								{#each pulse.certificates as alert (`${alert.object.namespace}/${alert.object.name}`)}
									<li>
										<button onclick={() => openCertificate(alert)} title={t('Open {name}', { name: alert.object.name })}>
											<span class="when" title={alert.notAfter ?? undefined}>{alert.notAfter ? relative(alert.notAfter, now) : '—'}</span>
											<span class="tag {alert.health === 'failed' ? 'bad' : 'warn'}">{alert.health === 'failed' ? t('Critical') : t('Warning')}</span>
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
							<h2 id="trend-title">{t('Usage trend')}</h2>
							<span class="muted">{namespace ? t('This namespace') : t('Cluster-wide')}</span>
						</div>
						{#if cpuTrend.length < 2 && memoryTrend.length < 2 && (fill(pulse.cpu) || fill(pulse.memory))}
							<!-- One line until there is a trend to draw, not two empty charts. -->
							<p class="trend-wait" role="status"><span class="loader" aria-hidden="true"></span>{t('The trend appears after a few updates.')}</p>
						{:else}
							<div class="trends">
								{@render trend('CPU', pulse.cpu, cpuTrend, cores)}
								{@render trend(t('Memory'), pulse.memory, memoryTrend, memory)}
							</div>
						{/if}
						<p class="note">
							{trendWindow ? t('The last hour from Prometheus, measured use.') : t('Kept for this session; Kubri stores no history.')}
						</p>
					</section>
				</div>
			</div>
			<OtherContexts />
		</div>
	{:else if !error}
		<div class="loading">{t('Loading the overview…')}</div>
	{/if}
</section>

{#snippet countCard(index: number, label: string, icon: IconName, kind: KindPulse | undefined, scope: string)}
	<button class="kpi" class:cursor={marked(index)} data-tile={index} onclick={() => activate(index)}>
		<span class="kpi-label">{label}<Icon name={icon} /></span>
		{#if !kind}
			<span class="denied">{t('Not shown')}</span>
			<span class="kpi-note">{index === 1 ? t('Nodes show for all namespaces') : t('Not listed')}</span>
		{:else if kind.error}
			<span class="denied" title={kind.error}>{t('No access')}</span>
			<span class="kpi-note">{scope}</span>
		{:else}
			<span class="kpi-value">{kind.ok}<small> / {kind.total}</small></span>
			<span class="kpi-note">
				{kind.warning || kind.failed
					? [kind.warning && t('{n} warning', { n: kind.warning }), kind.failed && t('{n} failing', { n: kind.failed })]
							.filter(Boolean)
							.join(' · ')
					: kind.total
						? `${scope} · ${t('all healthy')}`
						: `${scope} · ${t('none')}`}
			</span>
			<span class="bar" aria-hidden="true">
				{#each [['ok', kind.ok], ['warning', kind.warning], ['failed', kind.failed]] as [health, count] (health)}
					{#if Number(count) > 0}<span class="segment {health}" style:flex-grow={count}></span>{/if}
				{/each}
			</span>
		{/if}
	</button>
{/snippet}

{#snippet usageCard(index: number, label: string, icon: IconName, u: Utilization, format: (v: number) => string, resource: 'cpu' | 'memory')}
	{@const r = room(u)}
	{@const waiting = (pulse?.waiting ?? []).filter((w) => w[resource] > 0)}
	<button
		class="kpi"
		class:cursor={marked(index)}
		data-tile={index}
		onclick={() => activate(index)}
		title={r?.reserved != null
			? t('Reserved is what running pods ask for: the scheduler sets it aside, used or not. A new pod starts only where enough is left.')
			: namespace
				? t('Open pods')
				: t('Open nodes')}
	>
		<span class="kpi-label">{label}<Icon name={icon} /></span>
		{#if r}
			<span class="kpi-value">{Math.round(r.value * 100)}<small>% {r.reserved !== null ? t('reserved') : t('used')}</small></span>
			<span class="kpi-note">
				{#if r.reserved !== null}
					{t('{reserved} of {capacity} reserved', { reserved: format(r.reserved), capacity: format(r.capacity) })}
				{:else}
					{t('{used} used of {capacity}', { used: format(r.used ?? 0), capacity: format(r.capacity) })}
				{/if}
			</span>
			{#if r.reserved !== null}
				<span class="kpi-note">
					{r.used !== null ? t('{used} in use right now', { used: format(r.used) }) : t('Use not measured: no metrics-server')}
				</span>
			{/if}
			{#if r.short && waiting.length}
				<span class="kpi-note short" title={waitingTitle(waiting, resource, format)}>
					<Icon name="warning" size={13} />
					{waiting.length === 1
						? t('{name} cannot start: it asks for {amount}', { name: waiting[0].object.name, amount: format(waiting[0][resource]) })
						: t('{n} pods cannot start: they ask for {amount}', { n: waiting.length, amount: format(r.waiting) })}
				</span>
			{/if}
			<span
				class="meter"
				role="meter"
				aria-valuemin="0"
				aria-valuemax="100"
				aria-valuenow={Math.round(r.value * 100)}
				aria-label="{label} {r.reserved !== null ? t('reserved') : t('used')}"
			>
				<span
					class="fill {r.short ? 'bad' : meterTone(r.value) === 'accent' ? '' : meterTone(r.value)}"
					style:width={percent(Math.min(r.value, 1))}
				></span>
				{#if r.reserved !== null && r.used !== null}
					<span class="request" style:left={percent(Math.min(r.used / r.capacity, 1))} title={t('In use')}></span>
				{/if}
			</span>
		{:else}
			<span class="denied">{t('Not available')}</span>
			<span class="kpi-note">{u.used === null ? t('Needs access to nodes and metrics-server') : t('Needs access to nodes')}</span>
		{/if}
	</button>
{/snippet}

{#snippet kindRow(kind: KindPulse, index: number)}
	<li>
		<button class:cursor={marked(index)} data-tile={index} onclick={() => activate(index)}>
			<span class="kind-name"><Icon name={KIND_ICONS[kind.kind] ?? 'resource'} />{kindLabel(kind)}</span>
			{#if kind.error}
				<span class="muted" title={kind.error}>{t('No access')}</span>
			{:else if kind.total === 0}
				<span class="muted">{t('None')}</span>
			{:else}
				<span class="kind-counts">
					{#if kind.failed}<span class="bad">{t('{n} failing', { n: kind.failed })}</span>{/if}
					{#if kind.warning}<span class="warn">{t('{n} warning', { n: kind.warning })}</span>{/if}
					<span><strong>{kind.ok}</strong> / {t('{total} healthy', { total: kind.total })}</span>
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
			<Sparkline samples={history} {format} label="{label} {level.measured ? t('used') : t('requested')}" {now} window={trendWindow} />
		{:else}
			<span class="muted">{t('Not available')}</span>
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
		grid-template-columns: repeat(5, minmax(0, 1fr));
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
		/* Lines left and above each tile; the box clips those of the first column and row. */
		box-shadow:
			-1px 0 0 var(--border),
			0 -1px 0 var(--border);
		background: none;
		text-align: left;
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
	.kpi-note.short {
		display: flex;
		gap: 5px;
		align-items: flex-start;
		color: var(--bad);
	}
	.kpi-note.short :global(svg) {
		flex: none;
		margin-top: 1px;
	}
	.health {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		font-size: 12.5px;
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
	.trend-wait {
		display: flex;
		align-items: center;
		gap: 10px;
		margin: 4px 0;
		color: var(--text-muted);
		font-size: 12.5px;
	}
	.loader {
		position: relative;
		width: 48px;
		height: 4px;
		overflow: hidden;
		border-radius: 2px;
		background: var(--surface-2);
	}
	.loader::after {
		content: '';
		position: absolute;
		inset: 0;
		width: 40%;
		border-radius: 2px;
		background: var(--accent);
		animation: slide 1.4s ease-in-out infinite;
	}
	@keyframes slide {
		from {
			transform: translateX(-100%);
		}
		to {
			transform: translateX(250%);
		}
	}
	@media (prefers-reduced-motion: reduce) {
		.loader::after {
			animation: none;
		}
	}
	.where {
		display: flex;
		flex-wrap: wrap;
		gap: 4px 12px;
		font-size: 11.5px;
		color: var(--text-muted);
	}
	.last-log {
		font-size: 12px;
		color: var(--text-muted);
		overflow-wrap: anywhere;
	}
	.last-log code {
		font-family: var(--mono);
		font-size: 11.5px;
		color: var(--text);
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
		/* The name keeps whole words; the counts wrap first. */
		grid-template-columns: minmax(min-content, 1fr) auto;
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
		overflow-wrap: break-word;
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
		/* Three counts above, the two meters below. */
		.kpis {
			grid-template-columns: repeat(6, minmax(0, 1fr));
		}
		.kpi {
			grid-column: span 2;
		}
		.kpi:nth-child(n + 4) {
			grid-column: span 3;
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
		.kpis {
			grid-template-columns: repeat(2, minmax(0, 1fr));
		}
		.kpi,
		.kpi:nth-child(n + 4) {
			grid-column: span 1;
			padding: 14px;
		}
		.kpi:nth-child(3) {
			grid-column: span 2;
		}
		.kpi-value {
			font-size: 24px;
		}
	}
</style>
