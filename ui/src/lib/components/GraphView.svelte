<script lang="ts">
	import { backend, type Graph, type GraphNode, type PathStep, type RequestPath } from '../api';
	import { errorMessage } from '../format';
	import { BOX, fit, layoutGraph, matches, neighborhood, type GroupBy } from '../graph';
	import { app, findResource } from '../state.svelte';
	import { kindTag, xrayResources } from '../xray';
	import HealthIcon from './HealthIcon.svelte';
	import PageHeader from './PageHeader.svelte';
	import SearchField from './SearchField.svelte';
	import Select from './Select.svelte';

	const REFRESH_MS = 15_000;

	let graph = $state.raw<Graph | null>(null);
	let error = $state<string | null>(null);
	let filter = $state('');
	let problemsOnly = $state(false);
	const namespace = $derived(app.namespace || null);
	let groupBy = $state<GroupBy>(app.namespace ? 'app' : 'namespace');
	let hovered = $state<string | null>(null);
	let selectedPath = $state<RequestPath | null>(null);

	$effect(() => {
		const ns = namespace;
		let cancelled = false;
		let loading = false;
		graph = null;
		error = null;
		const load = async () => {
			if (loading) return;
			loading = true;
			try {
				const next = await backend.graph(ns);
				if (!cancelled) {
					graph = next;
					error = null;
				}
			} catch (err) {
				if (!cancelled) error = errorMessage(err);
			} finally {
				loading = false;
			}
		};
		load();
		const timer = setInterval(() => document.hidden || load(), REFRESH_MS);
		return () => {
			cancelled = true;
			clearInterval(timer);
		};
	});

	const layout = $derived(graph ? layoutGraph(graph, groupBy) : null);
	const issues = $derived(graph?.nodes.filter((n) => n.health !== 'ok' && n.kind !== 'Node').length ?? 0);
	const broken = $derived(graph?.paths.filter((p) => p.broken).length ?? 0);

	/** What is lit: the neighbors of the hovered object, the steps of the chosen path, or the filter's matches. */
	const lit = $derived.by((): Set<string> | null => {
		if (!graph) return null;
		const edges = layout?.edges.map((e) => e.edge) ?? [];
		if (hovered) return neighborhood(edges, hovered);
		if (selectedPath) {
			const ids = new Set([...selectedPath.steps, ...selectedPath.pods].flatMap((s) => (s.id ? [s.id] : [])));
			// The workloads of its pods, which the Service points at.
			for (const e of graph.edges) if (e.kind === 'owns' && ids.has(e.to)) ids.add(e.from);
			return ids;
		}
		if (filter.trim() || problemsOnly) {
			return new Set(graph.nodes.filter((n) => matches(n, filter) && (!problemsOnly || n.health !== 'ok')).map((n) => n.id));
		}
		return null;
	});

	// Pan and zoom.
	let view = $state({ scale: 1, x: 0, y: 0 });
	let frame = $state<HTMLDivElement>();
	let fitted = false;
	$effect(() => {
		if (!layout || !frame || fitted) return;
		fitted = true;
		view = fit(layout, frame.clientWidth, frame.clientHeight, true);
	});
	$effect(() => {
		void groupBy;
		void namespace;
		fitted = false;
	});

	function zoom(factor: number, cx?: number, cy?: number) {
		if (!frame) return;
		const x = cx ?? frame.clientWidth / 2;
		const y = cy ?? frame.clientHeight / 2;
		const scale = Math.min(2.5, Math.max(0.15, view.scale * factor));
		const k = scale / view.scale;
		view = { scale, x: x - (x - view.x) * k, y: y - (y - view.y) * k };
	}

	function onwheel(e: WheelEvent) {
		e.preventDefault();
		if (e.ctrlKey || e.metaKey || Math.abs(e.deltaY) > Math.abs(e.deltaX) * 2) {
			const rect = frame!.getBoundingClientRect();
			zoom(Math.exp(-e.deltaY * (e.ctrlKey ? 0.01 : 0.0015)), e.clientX - rect.left, e.clientY - rect.top);
		} else {
			view = { ...view, x: view.x - e.deltaX, y: view.y - e.deltaY };
		}
	}

	let drag: { x: number; y: number; vx: number; vy: number; moved: boolean } | null = null;
	function onpointerdown(e: PointerEvent) {
		if (e.button !== 0) return;
		drag = { x: e.clientX, y: e.clientY, vx: view.x, vy: view.y, moved: false };
	}
	function onpointermove(e: PointerEvent) {
		if (!drag) return;
		const dx = e.clientX - drag.x;
		const dy = e.clientY - drag.y;
		if (!drag.moved && Math.hypot(dx, dy) < 4) return;
		if (!drag.moved) frame?.setPointerCapture(e.pointerId);
		drag.moved = true;
		view = { ...view, x: drag.vx + dx, y: drag.vy + dy };
	}
	function onpointerup() {
		// A drag that moved is not a click on what it ended over.
		const moved = drag?.moved;
		drag = null;
		if (moved) suppressClick = true;
	}
	let suppressClick = false;

	function open(node: { kind: string; name: string; namespace: string | null; resource?: GraphNode['resource'] }) {
		if (suppressClick) {
			suppressClick = false;
			return;
		}
		const ref = node.resource;
		const resource = ref && findResource(app.resources, ref);
		if (resource) app.showDetails(resource, { resource: ref, namespace: node.namespace, name: node.name });
		else app.toast(`${node.kind} ${node.name} is not there`, 'bad');
	}

	function openStep(step: PathStep, path: RequestPath) {
		const node = step.id ? graph?.nodes.find((n) => n.id === step.id) : undefined;
		if (node) open(node);
		else if (step.kind === 'EndpointSlice') {
			const resource = app.resources.find((r) => r.group === 'discovery.k8s.io' && r.plural === 'endpointslices');
			if (resource) app.open(resource, path.namespace ?? undefined, `-l kubernetes.io/service-name=${path.steps[1]?.name ?? ''}`);
		}
	}

	function showTrees() {
		const resource = app.xrayResource ?? xrayResources(app.resources)[0];
		if (resource) app.showXray(resource);
	}

	const where = (p: RequestPath) => `${p.host ?? '*'}${p.path.startsWith('/') ? '' : ' '}${p.path}`;
</script>

<section class="page">
	<PageHeader
		crumbs={['Explore', 'Topology', 'Graph']}
		title="Topology graph"
		count={graph?.nodes.length}
		subtitle="How the objects {namespace ? `in ${namespace}` : 'in all namespaces'} connect, from Ingresses to the nodes their pods run on"
	>
		{#if issues}<span class="tag bad"><HealthIcon health="failed" />{issues} with issues</span>{/if}
		{#snippet tools()}
			<button class="btn" onclick={showTrees} title="XRay trees, one per object">Trees</button>
		{/snippet}
	</PageHeader>

	{#if error}
		<div class="notice" role="alert">Could not build the graph: {error}</div>
	{/if}
	{#if graph?.missing.length}
		<p class="muted small">Not shown, since they could not be listed: {graph.missing.join(', ')}.</p>
	{/if}

	<div class="card">
		<div class="toolbar">
			<label>
				<span>Group by</span>
				<Select
					bind:value={groupBy}
					options={[
						{ value: 'namespace', label: 'Namespace' },
						{ value: 'app', label: 'Application' },
						{ value: 'node', label: 'Node' }
					]}
					compact
				/>
			</label>
			<SearchField bind:value={filter} label="Find in the graph" placeholder="Find by name, kind or state…" key="" />
			<label class="check"><input type="checkbox" bind:checked={problemsOnly} /> Problems</label>
			<span class="grow"></span>
			<div class="zoom" role="group" aria-label="Zoom">
				<button class="btn" onclick={() => zoom(1 / 1.25)} aria-label="Zoom out">−</button>
				<button class="btn" onclick={() => layout && frame && (view = fit(layout, frame.clientWidth, frame.clientHeight))}>Fit</button>
				<button class="btn" onclick={() => zoom(1.25)} aria-label="Zoom in">+</button>
			</div>
		</div>

		<!-- svelte-ignore a11y_no_static_element_interactions -->
		<div class="frame" bind:this={frame} {onwheel} {onpointerdown} {onpointermove} {onpointerup} onpointercancel={onpointerup}>
			{#if !graph && !error}
				<p class="muted center">Reading the objects…</p>
			{:else if layout && !layout.boxes.length}
				<p class="muted center">Nothing here to draw.</p>
			{:else if layout}
				<svg width="100%" height="100%" role="img" aria-label="Topology graph">
					<g transform="translate({view.x} {view.y}) scale({view.scale})">
						{#each layout.bands as band (band.label)}
							<rect class="band" x={band.x} y={band.y} width={band.width} height={band.height} rx="10" />
							<text class="band-label" x={band.x + 12} y={band.y + 20}>
								{band.label}{band.issues ? ` · ${band.issues} with issues` : ''}
							</text>
						{/each}
						{#each layout.edges as { edge, d } (edge.from + '>' + edge.to)}
							<path class="edge {edge.kind}" class:dim={lit && !(lit.has(edge.from) && lit.has(edge.to))} {d} />
						{/each}
						{#each layout.boxes as { node, x, y } (node.id)}
							<!-- svelte-ignore a11y_click_events_have_key_events -->
							<g
								class="box {node.health}"
								class:dim={lit && !lit.has(node.id)}
								class:missing={!node.resource}
								transform="translate({x} {y})"
								role="button"
								tabindex="0"
								aria-label="{node.kind} {node.name}: {node.detail || node.health}"
								onclick={() => open(node)}
								onkeydown={(e) => e.key === 'Enter' && open(node)}
								onpointerenter={() => (hovered = node.id)}
								onpointerleave={() => (hovered = null)}
							>
								<title>{node.kind} {node.namespace ? `${node.namespace}/` : ''}{node.name}{node.detail ? `\n${node.detail}` : ''}</title>
								<rect width={BOX.width} height={BOX.height} rx="7" />
								<rect class="stripe" width="4" height={BOX.height} rx="2" />
								<text class="kind" x="12" y="16">{kindTag(node.kind)}</text>
								<text class="name" x="12" y="32">{node.name.length > 26 ? `${node.name.slice(0, 25)}…` : node.name}</text>
								{#if node.health !== 'ok'}
									<text class="detail" x={BOX.width - 8} y="16" text-anchor="end">
										{node.detail.length > 22 ? `${node.detail.slice(0, 21)}…` : node.detail}
									</text>
								{/if}
							</g>
						{/each}
					</g>
				</svg>
				<div class="legend" aria-hidden="true">
					<span><i class="line routes"></i>routes to</span>
					<span><i class="line selects"></i>selects</span>
					<span><i class="line owns"></i>owns</span>
					<span><i class="line uses"></i>uses</span>
				</div>
			{/if}
		</div>
	</div>

	{#if graph?.paths.length}
		<div class="card paths">
			<h2>
				Request paths
				{#if broken}<span class="tag bad"><HealthIcon health="failed" />{broken} broken</span>{/if}
			</h2>
			<ul>
				{#each graph.paths as p, i (i)}
					<li
						class:chosen={selectedPath === p}
						onpointerenter={() => (selectedPath = p)}
						onpointerleave={() => (selectedPath = null)}
					>
						<span class="where mono" title="Ingress {p.namespace ? `${p.namespace}/` : ''}{p.ingress}">
							{#if !namespace && p.namespace}<span class="muted">{p.namespace} · </span>{/if}{where(p)}
						</span>
						<span class="chain">
							{#each p.steps as s, j (j)}
								{#if j}<span class="link" class:broken={s.health === 'failed'}>→</span>{/if}
								<button class="step {s.health}" onclick={() => openStep(s, p)} title={s.detail}>
									<HealthIcon health={s.health} />
									<span class="muted">{kindTag(s.kind === 'EndpointSlice' ? 'Endpoints' : s.kind)}</span>
									{s.name}
								</button>
							{/each}
							{#if p.pods.length}
								<span class="link" class:broken={!p.pods.some((x) => x.health === 'ok')}>→</span>
								{#each p.pods.slice(0, 3) as pod (pod.name)}
									<button class="step {pod.health}" onclick={() => openStep(pod, p)} title={pod.detail}>
										<HealthIcon health={pod.health} />
										{pod.name}
									</button>
								{/each}
								{#if p.pods.length > 3}<span class="muted">+{p.pods.length - 3}</span>{/if}
							{/if}
						</span>
						{#if p.broken}<span class="why">{p.broken}</span>{/if}
					</li>
				{/each}
			</ul>
		</div>
	{/if}
</section>

<style>
	.page {
		display: flex;
		flex-direction: column;
		gap: 12px;
		min-height: 0;
		min-width: 0;
		padding: 0 24px 20px;
	}
	.card {
		display: flex;
		flex-direction: column;
		min-height: 0;
		border: 1px solid var(--border);
		border-radius: 10px;
		background: var(--surface-1);
		overflow: hidden;
	}
	.card:first-of-type {
		flex: 1;
		min-height: 360px;
	}
	.toolbar {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 10px;
		padding: 10px 12px;
		border-bottom: 1px solid var(--border);
		font-size: 13px;
	}
	.toolbar label {
		display: flex;
		align-items: center;
		gap: 6px;
		color: var(--text-muted);
	}
	.toolbar :global(.search) {
		width: 260px;
		height: 32px;
	}
	.grow {
		flex: 1;
	}
	.zoom {
		display: flex;
		gap: 4px;
	}
	.zoom .btn {
		min-width: 32px;
	}
	.frame {
		position: relative;
		flex: 1;
		min-height: 300px;
		overflow: hidden;
		cursor: grab;
		touch-action: none;
		background: var(--bg);
	}
	.frame:active {
		cursor: grabbing;
	}
	svg {
		display: block;
		user-select: none;
	}
	.center {
		padding: 24px;
		text-align: center;
	}
	.band {
		fill: var(--surface-1);
		stroke: var(--border);
	}
	.band-label {
		fill: var(--text-muted);
		font-size: 12px;
		font-weight: 600;
	}
	.edge {
		fill: none;
		stroke: var(--text-muted);
		stroke-width: 1.4;
		opacity: 0.55;
		transition: opacity 0.12s;
	}
	.edge.routes {
		stroke: var(--accent);
		stroke-width: 2;
		opacity: 0.8;
	}
	.edge.selects {
		stroke: var(--accent);
	}
	.edge.uses {
		stroke-dasharray: 4 3;
	}
	.edge.runsOn {
		stroke-dasharray: 1 3;
	}
	.edge.dim {
		opacity: 0.08;
	}
	.box {
		cursor: pointer;
		transition: opacity 0.12s;
	}
	.box rect:first-of-type {
		fill: var(--surface-1);
		stroke: var(--border);
	}
	.box:hover rect:first-of-type,
	.box:focus-visible rect:first-of-type {
		stroke: var(--accent);
		stroke-width: 2;
	}
	.box:focus-visible {
		outline: none;
	}
	.stripe {
		fill: var(--ok);
	}
	.box.warning .stripe {
		fill: var(--warn);
	}
	.box.failed .stripe {
		fill: var(--bad);
	}
	.box.failed rect:first-of-type {
		stroke: color-mix(in srgb, var(--bad) 60%, var(--border));
		fill: color-mix(in srgb, var(--bad) 7%, var(--surface-1));
	}
	.box.missing rect:first-of-type {
		stroke-dasharray: 4 3;
	}
	.box.dim {
		opacity: 0.25;
	}
	.kind {
		fill: var(--text-muted);
		font-family: var(--mono);
		font-size: 10.5px;
	}
	.name {
		fill: var(--text);
		font-family: var(--mono);
		font-size: 12px;
	}
	.detail {
		fill: var(--bad);
		font-size: 10.5px;
	}
	.box.warning .detail {
		fill: var(--warn);
	}
	.legend {
		position: absolute;
		right: 10px;
		bottom: 8px;
		display: flex;
		gap: 12px;
		padding: 4px 8px;
		border-radius: 6px;
		background: color-mix(in srgb, var(--surface-1) 85%, transparent);
		color: var(--text-muted);
		font-size: 11.5px;
	}
	.legend span {
		display: flex;
		align-items: center;
		gap: 5px;
	}
	.line {
		width: 18px;
		border-top: 2px solid var(--text-muted);
	}
	.line.routes,
	.line.selects {
		border-color: var(--accent);
	}
	.line.uses {
		border-top-style: dashed;
	}
	.paths h2 {
		display: flex;
		align-items: center;
		gap: 8px;
		margin: 0;
		padding: 10px 12px;
		border-bottom: 1px solid var(--border);
		font-size: 14px;
	}
	.paths ul {
		max-height: 240px;
		margin: 0;
		padding: 0;
		overflow: auto;
		list-style: none;
	}
	.paths li {
		display: grid;
		grid-template-columns: minmax(120px, 220px) 1fr;
		gap: 4px 12px;
		align-items: center;
		padding: 8px 12px;
		border-bottom: 1px solid var(--border-subtle);
		font-size: 12.5px;
	}
	.paths li.chosen {
		background: var(--surface-2);
	}
	.where {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.chain {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 4px;
	}
	.step {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		padding: 2px 8px;
		border: 1px solid var(--border);
		border-radius: 999px;
		background: var(--surface-1);
		font-family: var(--mono);
		font-size: 12px;
	}
	.step.failed {
		border-color: var(--bad);
		color: var(--bad);
	}
	.link {
		color: var(--text-muted);
	}
	.link.broken {
		color: var(--bad);
		font-weight: 700;
	}
	.why {
		grid-column: 2;
		color: var(--bad);
	}
	.tag {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		padding: 2px 8px;
		border-radius: 999px;
		font-size: 12px;
		font-weight: 600;
	}
	.tag.bad {
		background: color-mix(in srgb, var(--bad) 12%, transparent);
		color: var(--bad);
	}
	.notice {
		padding: 10px 12px;
		border-radius: 8px;
		background: color-mix(in srgb, var(--bad) 10%, transparent);
		color: var(--bad);
	}
	.muted {
		color: var(--text-muted);
	}
	.small {
		margin: 0;
		font-size: 12px;
	}
	.mono {
		font-family: var(--mono);
	}
</style>
