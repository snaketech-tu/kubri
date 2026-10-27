<script lang="ts">
	import type { KubeObject, ResourceInfo } from '../api';
	import { age } from '../format';
	import { LiveList } from '../watch.svelte';

	let { resource, namespace }: { resource: ResourceInfo; namespace: string } = $props();

	const list = new LiveList();
	let filter = $state('');
	let now = $state(Date.now());

	$effect(() => {
		return list.start(resource, resource.namespaced && namespace ? namespace : null);
	});

	$effect(() => {
		const timer = setInterval(() => (now = Date.now()), 1000);
		return () => clearInterval(timer);
	});

	const showNamespace = $derived(resource.namespaced && !namespace);
	const rows = $derived(
		filter ? list.items.filter((o) => o.metadata.name.includes(filter)) : list.items
	);

	// A short status for the common kinds; step 2 replaces this with server-side table columns.
	function status(obj: KubeObject): string {
		const s = obj.status as Record<string, any> | undefined;
		if (!s) return '';
		if (obj.metadata.deletionTimestamp) return 'Terminating';
		if (resource.kind === 'Pod') {
			const waiting = s.containerStatuses?.find((c: any) => c.state?.waiting)?.state.waiting.reason;
			return waiting ?? s.phase ?? '';
		}
		if (s.phase) return s.phase;
		if (typeof s.readyReplicas === 'number' || typeof s.replicas === 'number') {
			return `${s.readyReplicas ?? 0}/${(obj.spec as any)?.replicas ?? s.replicas ?? 0} ready`;
		}
		return '';
	}

	function tone(text: string) {
		if (/^(Running|Active|Bound|Succeeded|Complete)/.test(text)) return 'ok';
		if (/(Error|Failed|BackOff|Lost|Unknown)/.test(text)) return 'bad';
		if (text) return 'warn';
		return '';
	}
</script>

<section>
	<header>
		<div class="title">
			<h1>{resource.kind}</h1>
			<span class="count">{rows.length}</span>
			<code>{resource.group ? `${resource.group}/` : ''}{resource.version}</code>
		</div>
		<input class="filter" bind:value={filter} placeholder="Filter by name…" />
	</header>

	{#if list.error}
		<div class="banner">{list.error}</div>
	{/if}

	<div class="table-wrap">
		<table>
			<thead>
				<tr>
					{#if showNamespace}<th>Namespace</th>{/if}
					<th>Name</th>
					<th>Status</th>
					<th class="age">Age</th>
				</tr>
			</thead>
			<tbody>
				{#each rows as obj (obj.metadata.uid ?? obj.metadata.name)}
					{@const st = status(obj)}
					<tr>
						{#if showNamespace}<td class="muted">{obj.metadata.namespace}</td>{/if}
						<td class="name">{obj.metadata.name}</td>
						<td><span class="status {tone(st)}">{st}</span></td>
						<td class="age muted">{age(obj.metadata.creationTimestamp, now)}</td>
					</tr>
				{/each}
			</tbody>
		</table>
		{#if list.loading}
			<p class="empty">Loading…</p>
		{:else if rows.length === 0}
			<p class="empty">No {resource.kind} objects{namespace ? ` in ${namespace}` : ''}.</p>
		{/if}
	</div>
</section>

<style>
	section {
		display: flex;
		flex-direction: column;
		min-height: 0;
		min-width: 0;
	}
	header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 16px;
		padding: 16px 20px 12px;
	}
	.title {
		display: flex;
		align-items: baseline;
		gap: 10px;
	}
	h1 {
		margin: 0;
		font-size: 20px;
	}
	.count {
		padding: 1px 8px;
		border-radius: 999px;
		background: var(--surface-2);
		font-size: 12px;
		font-weight: 600;
	}
	code {
		color: var(--text-muted);
		font-size: 12px;
	}
	.filter {
		width: 260px;
	}
	.banner {
		margin: 0 20px 12px;
		padding: 8px 12px;
		border-radius: 8px;
		background: color-mix(in srgb, var(--bad) 15%, transparent);
		color: var(--bad);
	}
	.table-wrap {
		flex: 1;
		overflow: auto;
		padding: 0 20px 20px;
	}
	table {
		width: 100%;
		border-collapse: collapse;
		font-size: 13px;
	}
	th {
		position: sticky;
		top: 0;
		padding: 8px 10px;
		background: var(--bg);
		border-bottom: 1px solid var(--border);
		text-align: left;
		font-size: 11px;
		font-weight: 600;
		letter-spacing: 0.05em;
		text-transform: uppercase;
		color: var(--text-muted);
	}
	td {
		padding: 7px 10px;
		border-bottom: 1px solid var(--border-subtle);
		white-space: nowrap;
	}
	tbody tr:hover {
		background: var(--surface-1);
	}
	.name {
		font-family: var(--mono);
		font-size: 12.5px;
	}
	.muted {
		color: var(--text-muted);
	}
	.age {
		text-align: right;
		width: 70px;
	}
	.status.ok {
		color: var(--ok);
	}
	.status.warn {
		color: var(--warn);
	}
	.status.bad {
		color: var(--bad);
	}
	.empty {
		padding: 24px;
		text-align: center;
		color: var(--text-muted);
	}
</style>
