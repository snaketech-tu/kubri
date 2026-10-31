<script lang="ts">
	import { untrack } from 'svelte';
	import { backend, type SearchHit, type SearchResult } from '../api';
	import { age, errorMessage } from '../format';
	import { connectError } from '../fleet';
	import { parseSearch, unbounded } from '../search';
	import { app } from '../state.svelte';
	import Icon from './Icon.svelte';
	import PageHeader from './PageHeader.svelte';
	import SearchField from './SearchField.svelte';

	let input = $state<HTMLInputElement>();
	let text = $state(app.searchQuery);
	let found = $state<SearchResult | null>(null);
	let searched = $state('');
	let error = $state<string | null>(null);
	let busy = $state(false);
	let naming = $state<string | null>(null);
	let request = 0;
	let now = $state(Date.now());

	const parsed = $derived(parseSearch(text, app.resources, app.contexts));
	const saved = $derived(app.settings.searches.find((s) => s.query === searched.trim()));
	/** How many contexts the shown results come from a search of. */
	let contexts = $state(1);
	const elsewhere = $derived(contexts > 1 || !!found?.hits.some((h) => h.object.context !== app.cluster?.context));

	async function search(query = text) {
		const { request: req, problems } = parseSearch(query, app.resources, app.contexts);
		searched = query;
		app.searchQuery = query;
		if (problems.length) {
			error = problems.join('. ');
			found = null;
			return;
		}
		if (unbounded(req)) {
			error = null;
			found = null;
			return;
		}
		const mine = ++request;
		busy = true;
		error = null;
		try {
			const result = await backend.search(req);
			if (mine === request) {
				found = result;
				contexts = Math.max(1, req.contexts.length);
				now = Date.now();
			}
		} catch (err) {
			if (mine === request) {
				error = errorMessage(err);
				found = null;
			}
		} finally {
			if (mine === request) busy = false;
		}
	}

	// A saved search from the sidebar, or `:search …` from the palette, replaces what is typed.
	$effect(() => {
		const query = app.searchQuery;
		if (query === searched) return;
		untrack(() => {
			text = query;
			search(query);
		});
	});
	$effect(() => {
		input?.focus();
	});

	async function save() {
		const name = naming?.trim();
		if (!name) return;
		const rest = app.settings.searches.filter((s) => s.name !== name && s.query !== searched.trim());
		if (await app.setSavedSearches([...rest, { name, query: searched.trim() }])) {
			app.toast(`Saved ${name}`);
			naming = null;
		}
	}

	async function forget(name: string) {
		if (await app.setSavedSearches(app.settings.searches.filter((s) => s.name !== name))) app.toast(`Removed ${name}`);
	}

	const key = (h: SearchHit) => `${h.object.context}/${h.object.resource.group}/${h.object.resource.plural}/${h.object.namespace}/${h.object.name}`;
</script>

<section class="page">
	<PageHeader
		crumbs={['Search']} prefix="none"
		title={saved?.name ?? 'Search'}
		count={found?.hits.length}
		subtitle="Objects by name, label and field, in this cluster or in several"
	>
		{#snippet tools()}
			{#if found && !saved && naming === null}
				<button class="btn" onclick={() => (naming = searched.trim())}><Icon name="bookmark" size={14} /> Save search</button>
			{/if}
			{#if saved}
				<button class="btn" onclick={() => forget(saved.name)}>Remove from sidebar</button>
			{/if}
		{/snippet}
	</PageHeader>

	<form
		class="query"
		onsubmit={(e) => {
			e.preventDefault();
			search();
		}}
	>
		<SearchField bind:value={text} bind:input label="Search objects" placeholder="web kind:deploy,svc ns:shop -l tier=front status.phase!=Running ctx:*" key="" />
		<button class="btn primary" disabled={busy}>{busy ? 'Searching…' : 'Search'}</button>
	</form>
	{#if naming !== null}
		<form
			class="naming"
			onsubmit={(e) => {
				e.preventDefault();
				save();
			}}
		>
			<label>
				<span>Name in the sidebar</span>
				<!-- svelte-ignore a11y_autofocus -->
				<input bind:value={naming} autofocus spellcheck="false" />
			</label>
			<button class="btn primary" disabled={!naming.trim()}>Save</button>
			<button type="button" class="btn" onclick={() => (naming = null)}>Cancel</button>
		</form>
	{/if}

	{#if error}
		<div class="banner" role="alert">{error}</div>
	{:else if !found && !busy}
		<div class="help">
			<p>Type part of a name, or narrow it down:</p>
			<dl>
				<dt><code>web</code> <code>/^api-\d+$/</code></dt>
				<dd>Part of the name, or a regular expression between slashes</dd>
				<dt><code>kind:deploy,svc</code> <code>kind:all</code></dt>
				<dd>Kinds, named as in the palette; without it the common kinds, with <code>all</code> every kind you may list</dd>
				<dt><code>ns:shop</code> <code>-l app=web</code></dt>
				<dd>One namespace, and a label selector</dd>
				<dt><code>status.phase!=Running</code> <code>spec.replicas&gt;2</code></dt>
				<dd>A field compared with <code>= != ~ !~ &gt; &lt; &gt;= &lt;=</code>; <code>~</code> means contains</dd>
				<dt><code>ctx:*</code> <code>ctx:prod,staging</code></dt>
				<dd>Search every context, or some; without it only {app.cluster?.context ?? 'the connected one'}</dd>
			</dl>
			{#if searched.trim() && unbounded(parsed.request)}
				<p class="muted">Name, label, field, kind or namespace: a search needs at least one of them.</p>
			{/if}
		</div>
	{:else if found}
		<p class="source">
			Searched {found.searched} {found.searched === 1 ? 'list' : 'lists'}{contexts > 1 ? ` in ${contexts} contexts` : ''}.
			{#if found.truncated}<strong>Showing the first {found.hits.length}; narrow the search to see the rest.</strong>{/if}
		</p>
		{#if found.problems.length}
			<details class="problems">
				<summary><Icon name="warning" size={14} /> {found.problems.length} {found.problems.length === 1 ? 'list' : 'lists'} could not be searched</summary>
				<ul>
					{#each found.problems as p, i (i)}
						<li><strong>{p.context}{p.kind ? ` · ${p.kind}` : ''}</strong>: {connectError(p.message)}</li>
					{/each}
				</ul>
			</details>
		{/if}
		<div class="card">
			{#if found.hits.length}
				<div class="table-wrap">
					<table>
						<thead>
							<tr>
								<th>Name</th>
								<th>Kind</th>
								<th>Namespace</th>
								{#if elsewhere}<th>Context</th>{/if}
								<th class="numeric">Age</th>
							</tr>
						</thead>
						<tbody>
							{#each found.hits as h (key(h))}
								<tr>
									<td><button class="name" onclick={() => app.openObject(h.object, h.kind)}>{h.object.name}</button></td>
									<td>{h.kind}</td>
									<td class="mono">{h.object.namespace ?? '—'}</td>
									{#if elsewhere}<td class="mono">{h.object.context}</td>{/if}
									<td class="numeric">{age(h.created, app.snapshot?.taken ?? now)}</td>
								</tr>
							{/each}
						</tbody>
					</table>
				</div>
			{:else}
				<p class="empty">Nothing matches.</p>
			{/if}
		</div>
	{/if}
</section>

<style>
	.page {
		display: flex;
		flex-direction: column;
		min-height: 0;
		min-width: 0;
		padding: 0 24px 20px;
	}
	.query,
	.naming {
		display: flex;
		gap: 8px;
		margin-bottom: 12px;
	}
	.query :global(.search) {
		flex: 1;
	}
	.query :global(input) {
		font-family: var(--mono);
	}
	.naming {
		align-items: end;
	}
	.naming label {
		display: grid;
		gap: 4px;
		color: var(--text-muted);
		font-size: 12px;
	}
	.primary {
		border-color: var(--accent);
		background: var(--accent);
		color: var(--on-accent);
	}
	.help {
		color: var(--text-muted);
		font-size: 13px;
	}
	.help dl {
		display: grid;
		grid-template-columns: max-content 1fr;
		gap: 8px 16px;
		margin: 0 0 12px;
	}
	.help dt {
		display: flex;
		gap: 6px;
	}
	.help dd {
		margin: 0;
	}
	code,
	.mono {
		font-family: var(--mono);
		font-size: 12px;
	}
	code {
		padding: 1px 4px;
		border-radius: 4px;
		background: var(--surface-2);
		color: var(--text);
	}
	.source {
		margin: 0 0 10px;
		color: var(--text-muted);
		font-size: 12.5px;
	}
	.source strong {
		color: var(--text);
	}
	.problems {
		margin-bottom: 10px;
		color: var(--warn);
		font-size: 12.5px;
	}
	.problems summary {
		display: flex;
		align-items: center;
		gap: 6px;
		cursor: pointer;
	}
	.problems ul {
		margin: 6px 0 0;
		color: var(--text-muted);
		overflow-wrap: anywhere;
	}
	.card {
		display: flex;
		flex-direction: column;
		flex: 1;
		min-height: 0;
		border: 1px solid var(--border);
		border-radius: 10px;
		background: var(--surface-1);
		overflow: hidden;
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
		padding: 9px 10px;
		background: var(--surface-1);
		border-bottom: 1px solid var(--border);
		color: var(--text-muted);
		font-size: 12px;
		font-weight: 600;
		text-align: left;
	}
	td {
		padding: 7px 10px;
		border-bottom: 1px solid var(--border);
	}
	.numeric {
		text-align: right;
		white-space: nowrap;
	}
	.name {
		padding: 0;
		border: none;
		background: none;
		color: var(--accent);
		font: inherit;
		font-weight: 600;
		text-align: left;
		overflow-wrap: anywhere;
	}
	.name:hover {
		text-decoration: underline;
	}
	.empty {
		margin: 0;
		padding: 18px;
		color: var(--text-muted);
	}
	.banner {
		padding: 10px 12px;
		border-radius: 8px;
		background: color-mix(in srgb, var(--bad) 10%, transparent);
		color: var(--bad);
	}
	.muted {
		color: var(--text-muted);
	}
</style>
