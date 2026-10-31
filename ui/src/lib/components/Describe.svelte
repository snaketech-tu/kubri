<script lang="ts">
	import { copyLines } from '../actions';
	import type { EventInfo, KubeObject, Reference } from '../api';
	import { age } from '../format';
	import { secretValue } from '../pods';
	import { rulesOf } from '../rbac';
	import { cellTone } from '../status';
	import { app, findResource } from '../state.svelte';
	import EventTimeline from './EventTimeline.svelte';
	import RulesMatrix from './RulesMatrix.svelte';

	let {
		object,
		events,
		now,
		decoded = false,
		ondecode,
		oncontainer,
		shell = false,
		usedBy,
		onevents
	}: {
		object: KubeObject;
		events: EventInfo[];
		now: number;
		/** Shows the values of a Secret. */
		decoded?: boolean;
		ondecode?: () => void;
		/** Opens the logs or a shell of one container. */
		oncontainer?: (name: string, tab: 'logs' | 'shell') => void;
		/** Whether shells are on offer. */
		shell?: boolean;
		/** What uses a ConfigMap, Secret, PVC, ServiceAccount or PriorityClass. */
		usedBy?: Reference[] | { error: string } | 'loading';
		/** Shows all events, when there are more than the overview lists. */
		onevents?: () => void;
	} = $props();

	/** Events the overview lists before "All events". */
	const LATEST = 3;

	/** Shows a user in its list, with the cursor on it. */
	function showUser(ref: Reference) {
		const resource = findResource(app.resources, ref.resource);
		if (resource) app.openAt(resource, ref.namespace, ref.name);
	}

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

	/** Shows an object in its list, with the cursor on it, by its kind and API version. */
	function showObject(kind: string, apiVersion: string | undefined, namespace: string | null, name: string) {
		const group = apiVersion?.includes('/') ? apiVersion.split('/')[0] : apiVersion === undefined ? null : '';
		const kinds = app.resources.filter((r) => r.kind === kind && (group === null || r.group === group));
		const version = apiVersion?.split('/').pop();
		const resource = kinds.find((r) => r.version === version) ?? kinds[0];
		if (resource) app.openAt(resource, resource.namespaced ? namespace : null, name);
		else app.toast(`The cluster does not serve ${kind}`, 'bad');
	}

	/** A line of the facts, and where clicking its value goes. */
	type Fact = [label: string, value: string, go?: () => void];

	const facts = $derived.by(() => {
		const f: Fact[] = [];
		const ns = meta.namespace ?? null;
		if (ns) {
			f.push([
				'Namespace',
				ns,
				() => {
					app.namespace = ns;
					app.toast(`Namespace ${ns}`);
				}
			]);
		}
		f.push(['Created', `${age(meta.creationTimestamp, now)} ago · ${meta.creationTimestamp ?? ''}`]);
		if (meta.deletionTimestamp) f.push(['Deleting since', meta.deletionTimestamp]);
		const owners = meta.ownerReferences ?? [];
		if (owners.length === 1) {
			const [o] = owners;
			f.push(['Controlled by', `${o.kind}/${o.name}`, () => showObject(o.kind, o.apiVersion, ns, o.name)]);
		} else if (owners.length) {
			f.push(['Controlled by', owners.map((o) => `${o.kind}/${o.name}`).join(', ')]);
		}
		if (object.kind === 'Pod') {
			if (spec.nodeName) f.push(['Node', spec.nodeName, () => showObject('Node', 'v1', null, spec.nodeName)]);
			if (status.podIP) f.push(['Pod IP', status.podIP]);
			// "Running" next to a crash loop only misleads; the container state says it better.
			const waiting = [...(status.initContainerStatuses ?? []), ...(status.containerStatuses ?? [])].some(
				(c: any) => c.state?.waiting?.reason
			);
			if (status.phase && !waiting) f.push(['Phase', status.phase]);
			if (status.qosClass) f.push(['QoS class', status.qosClass]);
			const account = spec.serviceAccountName;
			if (account) f.push(['Service account', account, () => showObject('ServiceAccount', 'v1', ns, account)]);
		}
		if (typeof spec.replicas === 'number') {
			const parts = [`${spec.replicas} desired`];
			for (const key of ['readyReplicas', 'updatedReplicas', 'availableReplicas']) {
				if (typeof status[key] === 'number') parts.push(`${status[key]} ${key.replace('Replicas', '')}`);
			}
			f.push(['Replicas', parts.join(' · ')]);
		}
		if (spec.strategy?.type) f.push(['Strategy', spec.strategy.type]);
		const templated: any[] = spec.template?.spec?.containers ?? [];
		if (object.kind !== 'Pod' && templated.length) {
			f.push(['Containers', templated.map((c) => `${c.name}: ${c.image}`).join('\n')]);
		}
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
		if (roleRef?.kind) {
			const apiVersion = roleRef.apiGroup ? `${roleRef.apiGroup}/v1` : undefined;
			f.push(['Role', `${roleRef.kind}/${roleRef.name}`, () => showObject(roleRef.kind, apiVersion, ns, roleRef.name)]);
		}
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

	/** The data of a Secret, decoded from base64. */
	const secretData = $derived(
		object.kind === 'Secret'
			? Object.entries((object.data as Record<string, string> | undefined) ?? {}).map(([key, value]) => ({
					key,
					...secretValue(value)
				}))
			: []
	);

	const isRole = $derived(object.kind === 'Role' || object.kind === 'ClusterRole');
	const subjects = $derived<any[]>(Array.isArray((object as any).subjects) ? (object as any).subjects : []);

	const labels = $derived(Object.entries(meta.labels ?? {}));
	const annotations = $derived(
		Object.entries(meta.annotations ?? {}).filter(([k]) => k !== 'kubectl.kubernetes.io/last-applied-configuration')
	);
</script>

<div class="describe">
	<h3 class="first">At a glance</h3>
	<dl class="facts">
		{#each facts as [label, value, go] (label)}
			<dt>{label}</dt>
			<dd>
				{#if go}
					<button class="link" onclick={go} title={label === 'Namespace' ? `Show only namespace ${value}` : `Show ${value}`}>{value}</button>
				{:else}
					{value}
				{/if}
			</dd>
		{/each}
	</dl>

	<h3 class="with-tool">
		What happened
		{#if onevents && events.length > LATEST}
			<button onclick={onevents}>All {events.length} events</button>
		{/if}
	</h3>
	<EventTimeline {events} {now} limit={onevents ? LATEST : Infinity} />

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
			<thead><tr><th>Name</th><th>Image</th><th>Ready</th><th>Restarts</th><th>State</th><th></th></tr></thead>
			<tbody>
				{#each containers as c (c.name)}
					<tr>
						<td class="mono">{c.name}{#if c.init}<span class="tag">init</span>{/if}</td>
						<td class="mono wrap">{c.image}</td>
						<td class={c.ready ? 'ok' : 'warn'}>{c.ready ? 'yes' : 'no'}</td>
						<td class={c.restarts > 0 ? 'warn' : ''}>{c.restarts}</td>
						<td>{c.state}</td>
						<td class="actions">
							{#if oncontainer}
								<button onclick={() => oncontainer(c.name, 'logs')} title="Logs of {c.name}">Logs</button>
								{#if shell && !c.init}
									<button onclick={() => oncontainer(c.name, 'shell')} title="Shell in {c.name}">Shell</button>
								{/if}
							{/if}
						</td>
					</tr>
				{/each}
			</tbody>
		</table>
	{/if}

	{#if object.kind === 'Secret'}
		<h3 class="with-tool">
			Data
			{#if secretData.length && ondecode}
				<button onclick={ondecode} title="Show or hide the values (x)">{decoded ? 'Hide values' : 'Show values'}</button>
			{/if}
		</h3>
		{#if secretData.length}
			<dl class="facts small secret">
				{#each secretData as d (d.key)}
					<dt title={d.key}>{d.key}</dt>
					<dd>
						{#if !decoded}
							<span class="muted">{d.bytes} {d.bytes === 1 ? 'byte' : 'bytes'}</span>
						{:else if d.text === null}
							<span class="muted">binary, {d.bytes} {d.bytes === 1 ? 'byte' : 'bytes'}</span>
						{:else}
							<span class="value">{d.text}</span>
							<button class="copy" onclick={() => copyLines([d.text ?? ''], ['value', 'values'])} title="Copy the value">Copy</button>
						{/if}
					</dd>
				{/each}
			</dl>
		{:else}
			<p class="muted">No data.</p>
		{/if}
	{/if}

	{#if usedBy !== undefined}
		<h3>Used by</h3>
		{#if usedBy === 'loading'}
			<p class="muted">Looking for what uses it…</p>
		{:else if 'error' in usedBy}
			<p class="bad">{usedBy.error}</p>
		{:else if usedBy.length}
			<table>
				<thead><tr><th>Kind</th><th>Name</th><th>Namespace</th></tr></thead>
				<tbody>
					{#each usedBy as ref (ref.kind + ref.namespace + ref.name)}
						<tr>
							<td>{ref.kind}{#if ref.how}<span class="muted"> · {ref.how}</span>{/if}</td>
							<td class="mono"><button class="link" onclick={() => showUser(ref)} title="Show it in its list">{ref.name}</button></td>
							<td class="muted">{ref.namespace ?? ''}</td>
						</tr>
					{/each}
				</tbody>
			</table>
		{:else}
			<p class="muted">Nothing uses it.</p>
		{/if}
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

</div>

<style>
	.describe {
		padding: 16px 20px 24px;
		overflow: auto;
		font-size: 13px;
	}
	h3 {
		margin: 22px 0 10px;
		font-size: 14px;
		font-weight: 650;
	}
	h3.first {
		margin-top: 0;
	}
	.facts {
		display: grid;
		grid-template-columns: max-content 1fr;
		gap: 8px 20px;
		margin: 0;
	}
	.facts dt {
		color: var(--text-muted);
	}
	.facts dd {
		margin: 0;
		word-break: break-word;
		white-space: pre-line;
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
	.facts.secret dd {
		display: flex;
		align-items: baseline;
		gap: 8px;
		white-space: normal;
	}
	.value {
		white-space: pre-wrap;
		word-break: break-all;
	}
	h3.with-tool {
		display: flex;
		align-items: center;
		gap: 10px;
	}
	h3 button,
	.actions button,
	.copy {
		padding: 1px 8px;
		border: 1px solid var(--border);
		border-radius: 5px;
		background: var(--surface-1);
		color: var(--text);
		font-size: 11.5px;
		font-weight: 500;
		letter-spacing: normal;
		text-transform: none;
	}
	h3 button:hover,
	.actions button:hover,
	.copy:hover {
		border-color: var(--accent);
	}
	.actions {
		text-align: right;
	}
	.link {
		padding: 0;
		border: none;
		background: none;
		color: var(--accent-strong);
		font: inherit;
		text-align: left;
	}
	.link:hover {
		text-decoration: underline;
	}
	.actions button + button {
		margin-left: 4px;
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
