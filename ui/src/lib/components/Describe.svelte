<script lang="ts">
	import type { EventInfo, KubeObject } from '../api';
	import { age } from '../format';
	import { rulesOf } from '../rbac';
	import { cellTone } from '../status';
	import RulesMatrix from './RulesMatrix.svelte';

	let { object, events, now }: { object: KubeObject; events: EventInfo[]; now: number } = $props();

	const meta = $derived(object.metadata);
	const spec = $derived(object.spec ?? {});
	const status = $derived(object.status ?? {});
	const conditions = $derived<any[]>(Array.isArray(status.conditions) ? status.conditions : []);

	interface ContainerRow {
		name: string;
		image: string;
		ready: boolean;
		restarts: number;
		state: string;
		init: boolean;
	}

	const containers = $derived.by<ContainerRow[]>(() => {
		if (object.kind !== 'Pod') return [];
		const statuses = new Map<string, any>(
			[...(status.initContainerStatuses ?? []), ...(status.containerStatuses ?? [])].map((s: any) => [s.name, s])
		);
		const row = (c: any, init: boolean): ContainerRow => {
			const s = statuses.get(c.name);
			const state = s?.state ?? {};
			const key = Object.keys(state)[0] ?? '';
			const reason = state[key]?.reason;
			return {
				name: c.name,
				image: c.image,
				ready: !!s?.ready,
				restarts: s?.restartCount ?? 0,
				state: reason ? `${key} (${reason})` : key,
				init
			};
		};
		return [
			...(spec.initContainers ?? []).map((c: any) => row(c, true)),
			...(spec.containers ?? []).map((c: any) => row(c, false))
		];
	});

	const facts = $derived.by(() => {
		const f: [string, string][] = [];
		if (meta.namespace) f.push(['Namespace', meta.namespace]);
		f.push(['Created', `${age(meta.creationTimestamp, now)} ago · ${meta.creationTimestamp ?? ''}`]);
		if (meta.deletionTimestamp) f.push(['Deleting since', meta.deletionTimestamp]);
		const owners = meta.ownerReferences ?? [];
		if (owners.length) f.push(['Controlled by', owners.map((o) => `${o.kind}/${o.name}`).join(', ')]);
		if (object.kind === 'Pod') {
			if (spec.nodeName) f.push(['Node', spec.nodeName]);
			if (status.podIP) f.push(['Pod IP', status.podIP]);
			if (status.phase) f.push(['Phase', status.phase]);
			if (status.qosClass) f.push(['QoS class', status.qosClass]);
			if (spec.serviceAccountName) f.push(['Service account', spec.serviceAccountName]);
		}
		if (typeof spec.replicas === 'number') {
			const parts = [`${spec.replicas} desired`];
			for (const key of ['readyReplicas', 'updatedReplicas', 'availableReplicas']) {
				if (typeof status[key] === 'number') parts.push(`${status[key]} ${key.replace('Replicas', '')}`);
			}
			f.push(['Replicas', parts.join(' · ')]);
		}
		if (spec.strategy?.type) f.push(['Strategy', spec.strategy.type]);
		if (spec.selector?.matchLabels) {
			f.push(['Selector', Object.entries(spec.selector.matchLabels).map(([k, v]) => `${k}=${v}`).join(', ')]);
		}
		if (object.kind === 'Service') {
			if (spec.type) f.push(['Type', spec.type]);
			if (spec.clusterIP) f.push(['Cluster IP', spec.clusterIP]);
			if (Array.isArray(spec.ports)) {
				f.push([
					'Ports',
					spec.ports.map((p: any) => `${p.name ? p.name + ' ' : ''}${p.port}→${p.targetPort}/${p.protocol ?? 'TCP'}`).join(', ')
				]);
			}
		}
		if (object.kind === 'Node' && status.nodeInfo) {
			f.push(['Kubelet', status.nodeInfo.kubeletVersion]);
			f.push(['OS', `${status.nodeInfo.osImage} (${status.nodeInfo.architecture})`]);
			f.push(['Runtime', status.nodeInfo.containerRuntimeVersion]);
		}
		const roleRef = (object as any).roleRef;
		if (roleRef?.kind) f.push(['Role', `${roleRef.kind}/${roleRef.name}`]);
		const aggregation = (object as any).aggregationRule?.clusterRoleSelectors;
		if (Array.isArray(aggregation)) {
			const selectors = aggregation.map((s: any) =>
				Object.entries(s.matchLabels ?? {})
					.map(([k, v]) => `${k}=${v}`)
					.join(',')
			);
			f.push(['Aggregates roles labeled', selectors.join(' or ')]);
		}
		if (object.kind === 'ConfigMap' || object.kind === 'Secret') {
			const data = { ...((object.data as object) ?? {}), ...((object.binaryData as object) ?? {}) };
			f.push(['Keys', Object.keys(data).join(', ') || '(none)']);
		}
		return f;
	});

	const isRole = $derived(object.kind === 'Role' || object.kind === 'ClusterRole');
	const subjects = $derived<any[]>(Array.isArray((object as any).subjects) ? (object as any).subjects : []);

	const labels = $derived(Object.entries(meta.labels ?? {}));
	const annotations = $derived(
		Object.entries(meta.annotations ?? {}).filter(([k]) => k !== 'kubectl.kubernetes.io/last-applied-configuration')
	);
</script>

<div class="describe">
	<dl class="facts">
		{#each facts as [label, value] (label)}
			<dt>{label}</dt>
			<dd>{value}</dd>
		{/each}
	</dl>

	{#if labels.length}
		<h3>Labels</h3>
		<div class="chips">
			{#each labels as [k, v] (k)}
				<span class="chip"><span class="k">{k}</span>={v}</span>
			{/each}
		</div>
	{/if}

	{#if annotations.length}
		<h3>Annotations</h3>
		<dl class="facts small">
			{#each annotations as [k, v] (k)}
				<dt title={k}>{k}</dt>
				<dd title={v}>{v}</dd>
			{/each}
		</dl>
	{/if}

	{#if containers.length}
		<h3>Containers</h3>
		<table>
			<thead><tr><th>Name</th><th>Image</th><th>Ready</th><th>Restarts</th><th>State</th></tr></thead>
			<tbody>
				{#each containers as c (c.name)}
					<tr>
						<td class="mono">{c.name}{#if c.init}<span class="tag">init</span>{/if}</td>
						<td class="mono wrap">{c.image}</td>
						<td class={c.ready ? 'ok' : 'warn'}>{c.ready ? 'yes' : 'no'}</td>
						<td class={c.restarts > 0 ? 'warn' : ''}>{c.restarts}</td>
						<td>{c.state}</td>
					</tr>
				{/each}
			</tbody>
		</table>
	{/if}

	{#if isRole}
		<h3>Rules</h3>
		<RulesMatrix rules={rulesOf(object)} />
	{/if}

	{#if subjects.length}
		<h3>Subjects</h3>
		<table>
			<thead><tr><th>Kind</th><th>Name</th><th>Namespace</th></tr></thead>
			<tbody>
				{#each subjects as s, i (i)}
					<tr>
						<td>{s.kind}</td>
						<td class="mono">{s.name}</td>
						<td class="muted">{s.namespace ?? ''}</td>
					</tr>
				{/each}
			</tbody>
		</table>
	{/if}

	{#if conditions.length}
		<h3>Conditions</h3>
		<table>
			<thead><tr><th>Type</th><th>Status</th><th>Reason</th><th>Age</th><th>Message</th></tr></thead>
			<tbody>
				{#each conditions as c (c.type)}
					<tr>
						<td>{c.type}</td>
						<td class={cellTone('Status', c.status)}>{c.status}</td>
						<td>{c.reason ?? ''}</td>
						<td class="muted">{age(c.lastTransitionTime, now)}</td>
						<td class="wrap">{c.message ?? ''}</td>
					</tr>
				{/each}
			</tbody>
		</table>
	{/if}

	<h3>Events</h3>
	{#if events.length}
		<table>
			<thead><tr><th>Type</th><th>Reason</th><th>Age</th><th>From</th><th>Message</th></tr></thead>
			<tbody>
				{#each events as e, i (i)}
					<tr>
						<td class={e.kind === 'Warning' ? 'bad' : 'muted'}>{e.kind}</td>
						<td>{e.reason}</td>
						<td class="muted">{age(e.lastSeen, now)}{e.count > 1 ? ` (×${e.count})` : ''}</td>
						<td class="muted">{e.source}</td>
						<td class="wrap">{e.message}</td>
					</tr>
				{/each}
			</tbody>
		</table>
	{:else}
		<p class="muted">No recent events.</p>
	{/if}
</div>

<style>
	.describe {
		padding: 16px 20px 24px;
		overflow: auto;
		font-size: 13px;
	}
	h3 {
		margin: 20px 0 8px;
		font-size: 11px;
		font-weight: 600;
		letter-spacing: 0.06em;
		text-transform: uppercase;
		color: var(--text-muted);
	}
	.facts {
		display: grid;
		grid-template-columns: max-content 1fr;
		gap: 6px 16px;
		margin: 0;
	}
	.facts dt {
		color: var(--text-muted);
	}
	.facts dd {
		margin: 0;
		word-break: break-word;
	}
	.facts.small dt,
	.facts.small dd {
		font-family: var(--mono);
		font-size: 12px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.facts.small dt {
		max-width: 260px;
	}
	.chips {
		display: flex;
		flex-wrap: wrap;
		gap: 6px;
	}
	.chip {
		padding: 2px 8px;
		border-radius: 6px;
		background: var(--surface-2);
		font-family: var(--mono);
		font-size: 12px;
	}
	.chip .k {
		color: var(--text-muted);
	}
	table {
		width: 100%;
		border-collapse: collapse;
	}
	th {
		padding: 6px 8px;
		border-bottom: 1px solid var(--border);
		text-align: left;
		font-size: 11px;
		font-weight: 600;
		color: var(--text-muted);
	}
	td {
		padding: 6px 8px;
		border-bottom: 1px solid var(--border-subtle);
		vertical-align: top;
		white-space: nowrap;
	}
	td.wrap {
		white-space: normal;
		word-break: break-word;
	}
	.mono {
		font-family: var(--mono);
		font-size: 12px;
	}
	.tag {
		margin-left: 6px;
		padding: 0 5px;
		border-radius: 4px;
		background: var(--surface-2);
		font-size: 10px;
		color: var(--text-muted);
	}
	.muted {
		color: var(--text-muted);
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
