<script lang="ts">
	import { tick } from 'svelte';
	import { backend, type Grant, type Permissions, type Rbac, type SubjectInfo } from '../api';
	import { errorMessage } from '../format';
	import { forControl } from '../keys';
	import { filterSubjects, isAdmin, rbacResource, subjectLabel } from '../rbac';
	import { app } from '../state.svelte';
	import RulesMatrix from './RulesMatrix.svelte';

	const ME = 'me';
	const KINDS: [SubjectInfo['kind'], string][] = [
		['User', 'Users'],
		['Group', 'Groups'],
		['ServiceAccount', 'Service accounts']
	];

	let rbac = $state<Rbac | null>(null);
	let me = $state<Permissions | null>(null);
	let meError = $state<string | null>(null);
	let error = $state<string | null>(null);
	let filter = $state('');
	let selectedKey = $state(ME);
	let reloads = $state(0);
	let list = $state<HTMLDivElement>();
	let filterInput = $state<HTMLInputElement>();

	const namespace = $derived(app.namespace || null);

	$effect(() => {
		void reloads;
		const ns = namespace;
		let cancelled = false;
		error = null;
		meError = null;
		backend.rbac(ns).then(
			(r) => !cancelled && (rbac = r),
			(err) => !cancelled && (error = errorMessage(err))
		);
		backend.permissions(ns).then(
			(p) => !cancelled && (me = p),
			(err) => !cancelled && (meError = errorMessage(err))
		);
		return () => (cancelled = true);
	});

	const key = (s: SubjectInfo) => `${s.kind}/${s.namespace ?? ''}/${s.name}`;
	const visible = $derived(filterSubjects(rbac?.subjects ?? [], filter));
	const keys = $derived([ME, ...visible.map(key)]);
	const selected = $derived(visible.find((s) => key(s) === selectedKey) ?? null);

	$effect(() => {
		void selectedKey;
		tick().then(() => list?.querySelector('.item.active')?.scrollIntoView({ block: 'nearest' }));
	});

	function move(delta: number) {
		const i = Math.max(0, keys.indexOf(selectedKey));
		selectedKey = keys[Math.max(0, Math.min(keys.length - 1, i + delta))];
	}

	function open(kind: string, name: string, ns: string | null) {
		const resource = rbacResource(app.resources, kind);
		if (resource) app.showDetails(resource, { resource, namespace: resource.namespaced ? ns : null, name });
	}

	function scope(grant: Grant) {
		return grant.namespace ? `in ${grant.namespace}` : 'cluster-wide';
	}

	function onkeydown(e: KeyboardEvent) {
		if (app.overlayOpen) return;
		if (e.target === filterInput) {
			if (['Escape', 'Enter', 'ArrowDown'].includes(e.key)) {
				if (e.key === 'Escape') filter = '';
				filterInput?.blur();
				e.preventDefault();
			}
			return;
		}
		if (forControl(e) || e.ctrlKey || e.metaKey || e.altKey) return;
		const handled = (() => {
			switch (e.key) {
				case 'ArrowDown':
				case 'j':
					move(1);
					return true;
				case 'ArrowUp':
				case 'k':
					move(-1);
					return true;
				case 'g':
					move(-keys.length);
					return true;
				case 'G':
					move(keys.length);
					return true;
				case '/':
					filterInput?.focus();
					return true;
				case 'f':
					if (!app.details) return false;
					app.maximized = !app.maximized;
					return true;
				case 'Escape':
					if (app.details) {
						app.details = null;
						app.maximized = false;
					} else if (filter) {
						filter = '';
					}
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
		<h1>RBAC</h1>
		<span class="scope">{namespace ? `Role bindings in ${namespace}` : 'All namespaces'}</span>
		<span class="spacer"></span>
		<input class="filter" bind:this={filterInput} bind:value={filter} placeholder="Filter subjects…" />
		<button onclick={() => reloads++}>Reload</button>
	</header>

	{#if error}
		<div class="error">{error}</div>
	{/if}
	{#each rbac?.errors ?? [] as message (message)}
		<div class="error">Could not list {message}</div>
	{/each}

	<div class="split">
		<div class="list" bind:this={list} role="listbox" aria-label="Subjects">
			<button class="item me" class:active={selectedKey === ME} onclick={() => (selectedKey = ME)} role="option" aria-selected={selectedKey === ME}>
				<span class="name">You</span>
				<span class="muted">{me ? `rules in ${me.namespace}` : meError ? 'unknown' : '…'}</span>
			</button>
			{#each KINDS as [kind, title] (kind)}
				{@const items = visible.filter((s) => s.kind === kind)}
				{#if items.length}
					<h3>{title} <span class="count">{items.length}</span></h3>
					{#each items as s (key(s))}
						<button
							class="item"
							class:active={selectedKey === key(s)}
							onclick={() => (selectedKey = key(s))}
							role="option"
							aria-selected={selectedKey === key(s)}
						>
							<span class="name">{subjectLabel(s)}</span>
							{#if isAdmin(s.grants)}<span class="admin">admin</span>{/if}
							<span class="muted">{s.grants.length}</span>
						</button>
					{/each}
				{/if}
			{/each}
			{#if rbac && !visible.length}
				<p class="muted pad">{filter ? 'No subject matches the filter.' : 'No role bindings.'}</p>
			{/if}
		</div>

		<div class="detail">
			{#if selectedKey === ME}
				<h2>You <span class="muted">in {me?.namespace ?? app.namespace ?? 'default'}</span></h2>
				<p class="muted">What your credentials allow, including cluster-wide rules, as <code>kubectl auth can-i --list</code> shows it.</p>
				{#if meError}
					<div class="error inline">{meError}</div>
				{:else if me}
					{#if me.incomplete}
						<p class="note">The API server could not list every rule{me.error ? `: ${me.error}` : ''}. You may be allowed more than shown.</p>
					{/if}
					<RulesMatrix rules={me.rules} />
				{:else}
					<p class="muted">Loading…</p>
				{/if}
			{:else if selected}
				<h2>
					<span class="kind">{selected.kind}</span>
					{subjectLabel(selected)}
				</h2>
				{#each selected.grants as grant, i (i)}
					<div class="grant">
						<div class="grant-head">
							<button class="link" onclick={() => open(grant.bindingKind, grant.binding, grant.namespace)}>
								<span class="kind">{grant.bindingKind}</span>
								{grant.binding}
							</button>
							<span class="arrow">→</span>
							<button
								class="link"
								disabled={grant.roleState === 'missing'}
								onclick={() => open(grant.roleKind, grant.role, grant.namespace)}
							>
								<span class="kind">{grant.roleKind}</span>
								{grant.role}
							</button>
							<span class="pill" class:cluster={!grant.namespace}>{scope(grant)}</span>
						</div>
						{#if grant.roleState === 'missing'}
							<p class="bad">The {grant.roleKind} does not exist, so this binding grants nothing.</p>
						{:else if grant.roleState === 'unreadable'}
							<p class="muted">Roles could not be read, so the rules are unknown.</p>
						{:else}
							<RulesMatrix rules={grant.rules} />
						{/if}
					</div>
				{/each}
			{:else}
				<p class="muted">Pick a subject on the left.</p>
			{/if}
		</div>
	</div>
</section>

<style>
	section {
		display: flex;
		flex-direction: column;
		min-height: 0;
		min-width: 0;
		container-type: inline-size;
	}
	header {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 8px 10px;
		padding: 14px 20px 10px;
	}
	h1 {
		margin: 0;
		font-size: 20px;
	}
	.scope,
	.count {
		padding: 1px 8px;
		border-radius: 999px;
		background: var(--surface-2);
		font-size: 12px;
		font-weight: 600;
	}
	.spacer {
		flex: 1;
	}
	.filter {
		width: 200px;
		padding: 4px 8px;
		font-size: 13px;
	}
	header button {
		padding: 4px 10px;
		border: 1px solid var(--border);
		border-radius: 6px;
		background: var(--surface-1);
		font-size: 12px;
	}
	.error {
		margin: 0 20px 8px;
		padding: 8px 12px;
		border-radius: 8px;
		background: color-mix(in srgb, var(--bad) 12%, var(--surface-1));
		color: var(--bad);
	}
	.error.inline {
		margin: 8px 0;
	}
	.split {
		flex: 1;
		display: grid;
		grid-template-columns: minmax(220px, 300px) minmax(0, 1fr);
		min-height: 0;
		border-top: 1px solid var(--border-subtle);
	}
	.list {
		overflow-y: auto;
		padding: 8px;
		border-right: 1px solid var(--border-subtle);
	}
	/* Beside an open details panel there is no room for two columns: stack them. */
	@container (max-width: 720px) {
		.split {
			grid-template-columns: minmax(0, 1fr);
			grid-template-rows: minmax(0, 2fr) minmax(0, 3fr);
		}
		.list {
			border-right: none;
			border-bottom: 1px solid var(--border-subtle);
		}
	}
	h3 {
		display: flex;
		align-items: center;
		gap: 6px;
		margin: 14px 8px 4px;
		font-size: 11px;
		font-weight: 600;
		letter-spacing: 0.06em;
		text-transform: uppercase;
		color: var(--text-muted);
	}
	.item {
		display: flex;
		align-items: baseline;
		gap: 8px;
		width: 100%;
		padding: 5px 8px;
		border: none;
		border-radius: 6px;
		background: none;
		text-align: left;
		font-size: 13px;
	}
	.item:hover {
		background: var(--surface-2);
	}
	.item.active {
		background: color-mix(in srgb, var(--accent) 18%, transparent);
		color: var(--accent-strong);
	}
	.item .name {
		flex: 1;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		font-family: var(--mono);
		font-size: 12.5px;
	}
	.item.me .name {
		font-family: inherit;
		font-weight: 600;
	}
	.admin {
		padding: 0 6px;
		border-radius: 4px;
		background: color-mix(in srgb, var(--warn) 18%, transparent);
		color: var(--warn);
		font-size: 10.5px;
		font-weight: 600;
	}
	.muted {
		color: var(--text-muted);
		font-weight: 400;
	}
	.pad {
		padding: 8px;
	}
	.detail {
		overflow: auto;
		padding: 14px 20px 24px;
	}
	h2 {
		margin: 0 0 10px;
		font-family: var(--mono);
		font-size: 16px;
	}
	.kind {
		margin-right: 4px;
		color: var(--text-muted);
		font-family: Inter, system-ui, sans-serif;
		font-size: 11px;
		font-weight: 600;
		letter-spacing: 0.04em;
		text-transform: uppercase;
	}
	.grant {
		margin-bottom: 18px;
		padding: 12px 14px;
		border: 1px solid var(--border);
		border-radius: 10px;
		background: var(--surface-1);
	}
	.grant-head {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 6px 10px;
		margin-bottom: 8px;
	}
	.link {
		padding: 0;
		border: none;
		background: none;
		font-family: var(--mono);
		font-size: 13px;
		text-align: left;
	}
	.link:not(:disabled):hover {
		color: var(--accent-strong);
		text-decoration: underline;
	}
	.arrow {
		color: var(--text-muted);
	}
	.pill {
		margin-left: auto;
		padding: 1px 8px;
		border-radius: 999px;
		background: var(--surface-2);
		font-size: 11.5px;
	}
	.pill.cluster {
		background: color-mix(in srgb, var(--accent) 16%, transparent);
		color: var(--accent-strong);
	}
	.bad {
		color: var(--bad);
	}
	.note {
		padding: 6px 10px;
		border-radius: 6px;
		background: color-mix(in srgb, var(--warn) 12%, transparent);
	}
	code {
		font-family: var(--mono);
		font-size: 12px;
	}
</style>
