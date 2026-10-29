<script lang="ts">
	import { untrack } from 'svelte';
	import { parse, stringify } from 'yaml';
	import { backend, type HelmChange, type HelmChartRef, type HelmOutcome, type HelmReleaseDetails, type HubChart } from '../api';
	import { errorMessage } from '../format';
	import { chartLabel, hasValues } from '../helm';
	import { forControl } from '../keys';
	import { app } from '../state.svelte';
	import HelmCompare from './HelmCompare.svelte';
	import PageHeader from './PageHeader.svelte';
	import YamlEditor from './YamlEditor.svelte';

	let {
		release = null,
		oncancel,
		ondone
	}: {
		/** The release to upgrade; none installs a new one. */
		release?: HelmReleaseDetails | null;
		oncancel: () => void;
		ondone: (release: { namespace: string; name: string }) => void;
	} = $props();

	const install = $derived(!release);
	const initial = untrack(() => release);

	// The chart: for an upgrade, the one the release runs unless another is given.
	let source = $state<'stored' | 'other'>('stored');
	let chart = $state(initial?.chart ?? '');
	let repo = $state('');
	let version = $state('');
	let name = $state(initial?.name ?? '');
	let namespace = $state(initial?.namespace ?? (untrack(() => app.namespace) || 'default'));
	let createNamespace = $state(true);

	let editor = $state<YamlEditor>();
	let values = $state(initial && hasValues(initial.values) ? stringify(initial.values, { lineWidth: 0 }) : '');
	/** Bumped when the values are replaced from outside the editor. */
	let valuesVersion = $state(0);

	let query = $state('');
	let found = $state<HubChart[] | null>(null);
	let searching = $state(false);

	let busy = $state<string | null>(null);
	let error = $state<string | null>(null);
	let review = $state<{ outcome: HelmOutcome; values: unknown; key: string } | null>(null);

	function chartRef(): HelmChartRef | null {
		if (!install && source === 'stored') return null;
		return { chart: chart.trim(), repo: repo.trim() || null, version: version.trim() || null };
	}

	/** What a review was made for; a change to any of it needs another review. */
	function inputs(text: string) {
		return JSON.stringify([chartRef(), name.trim(), namespace.trim(), createNamespace, text]);
	}

	function readValues(): { text: string; parsed: unknown } | null {
		const text = editor?.text() ?? values;
		values = text;
		try {
			return { text, parsed: parse(text) ?? {} };
		} catch (err) {
			error = `The values are not valid YAML: ${errorMessage(err)}`;
			return null;
		}
	}

	function change(dryRun: boolean, text: string): HelmChange {
		return {
			namespace: namespace.trim(),
			name: name.trim(),
			chart: chartRef(),
			values: text,
			install,
			createNamespace: install && createNamespace,
			dryRun
		};
	}

	async function search(e: SubmitEvent) {
		e.preventDefault();
		if (!query.trim()) return;
		searching = true;
		error = null;
		try {
			found = await backend.helmSearch(query.trim());
		} catch (err) {
			error = `Artifact Hub could not be searched: ${errorMessage(err)}`;
			found = null;
		} finally {
			searching = false;
		}
	}

	function pick(c: HubChart) {
		chart = c.name;
		repo = c.repoUrl;
		version = c.version;
		if (!name.trim()) name = c.name;
		found = null;
		void loadDefaults();
	}

	async function loadDefaults() {
		const ref = chartRef();
		if (!ref?.chart) {
			error = 'Name the chart first.';
			return;
		}
		busy = 'Reading the chart…';
		error = null;
		try {
			const text = await backend.helmChartValues(ref);
			values = text.endsWith('\n') ? text : `${text}\n`;
			valuesVersion++;
		} catch (err) {
			error = errorMessage(err);
		} finally {
			busy = null;
		}
	}

	async function preview() {
		error = null;
		const read = readValues();
		if (!read) return;
		if (!name.trim() || !namespace.trim()) {
			error = 'Give the release a name and a namespace.';
			return;
		}
		const ref = chartRef();
		if (ref && !ref.chart) {
			error = 'Name the chart: a name with its repository URL, repo/chart for a repository helm knows, or oci://…';
			return;
		}
		busy = install ? 'Rendering the chart…' : 'Rendering the upgrade…';
		try {
			const outcome = await backend.helmChange(change(true, read.text));
			review = { outcome, values: read.parsed, key: inputs(read.text) };
		} catch (err) {
			error = errorMessage(err);
			review = null;
		} finally {
			busy = null;
		}
	}

	async function run() {
		if (!review) return;
		const text = editor?.text() ?? values;
		if (inputs(text) !== review.key) {
			error = 'Something changed since the review. Review again.';
			review = null;
			return;
		}
		busy = install ? 'Installing…' : 'Upgrading…';
		error = null;
		try {
			const outcome = await backend.helmChange(change(false, text));
			app.toast(
				install ? `Installed ${outcome.name}, revision ${outcome.revision}` : `Upgraded ${outcome.name} to revision ${outcome.revision}`,
				'ok'
			);
			ondone({ namespace: outcome.namespace, name: outcome.name });
		} catch (err) {
			error = errorMessage(err);
		} finally {
			busy = null;
		}
	}

	const blocked = $derived(app.readOnly ? 'Kubri is read-only for this context' : null);

	function onkeydown(e: KeyboardEvent) {
		if (app.overlayOpen) return;
		if (e.key === 'Escape' && !busy && !forControl(e)) {
			e.preventDefault();
			if (review) review = null;
			else oncancel();
		} else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 's' && !review) {
			e.preventDefault();
			void preview();
		}
	}
</script>

<svelte:window {onkeydown} />

<PageHeader
	crumbs={['Explore', { label: 'Helm', onclick: oncancel, title: 'Back (Esc)' }, install ? 'Install' : `${name} · upgrade`]}
	title={install ? 'Install a chart' : `Upgrade ${initial?.name}`}
	mono={!install}
	subtitle={install
		? 'From a repository, an OCI registry or Artifact Hub, with a dry run against the cluster first'
		: `In ${namespace} · now ${initial ? chartLabel(initial) : ''}, revision ${initial?.revision}`}
/>

{#if error}<div class="notice" role="alert">{error}</div>{/if}
{#if app.production && !review}
	<div class="notice warn">This is a production context. The review shows exactly what will change before anything does.</div>
{/if}

<div class="card">
	{#if !review}
		<div class="form">
			{#if install}
				<form class="row" onsubmit={search}>
					<label class="field grow">
						<span>Search Artifact Hub</span>
						<input bind:value={query} placeholder="nginx, postgres, cert-manager…" spellcheck="false" />
					</label>
					<button type="submit" disabled={searching || !query.trim()}>{searching ? 'Searching…' : 'Search'}</button>
				</form>
				{#if found}
					<ul class="found" aria-label="Charts found">
						{#each found.slice(0, 20) as c (c.url)}
							<li>
								<button class="pick" onclick={() => pick(c)}>
									<span class="mono">{c.repoName}/{c.name}</span>
									<span class="muted">{c.version}{c.appVersion ? ` · app ${c.appVersion}` : ''}</span>
									<span class="desc">{c.description}</span>
								</button>
							</li>
						{:else}
							<li class="muted">No charts found.</li>
						{/each}
					</ul>
				{/if}
			{:else}
				<div class="segmented" role="group" aria-label="Chart">
					<button aria-pressed={source === 'stored'} onclick={() => (source = 'stored')}>
						Same chart ({initial ? chartLabel(initial) : ''})
					</button>
					<button aria-pressed={source === 'other'} onclick={() => (source = 'other')}>Another chart or version</button>
				</div>
				{#if source === 'stored'}
					<p class="muted hint">
						Helm keeps the chart with the release, so only the values change. Subcharts are fetched again from their
						repositories.
					</p>
				{/if}
			{/if}
			{#if install || source === 'other'}
				<div class="row">
					<label class="field grow">
						<span>Chart</span>
						<input bind:value={chart} placeholder="nginx, bitnami/nginx or oci://registry/charts/nginx" spellcheck="false" />
					</label>
					<label class="field grow">
						<span>Repository URL</span>
						<input bind:value={repo} placeholder="https://charts.example.com (for a chart by name)" spellcheck="false" />
					</label>
					<label class="field">
						<span>Version</span>
						<input class="short" bind:value={version} placeholder="latest" spellcheck="false" />
					</label>
					<button onclick={loadDefaults} disabled={!!busy || !chart.trim()} title="Replace the values with the chart's defaults">
						Default values
					</button>
				</div>
			{/if}
			{#if install}
				<div class="row">
					<label class="field">
						<span>Release name</span>
						<input bind:value={name} spellcheck="false" />
					</label>
					<label class="field">
						<span>Namespace</span>
						<input bind:value={namespace} list="helm-namespaces" spellcheck="false" />
						<datalist id="helm-namespaces">
							{#each app.namespaces as n (n)}<option value={n}></option>{/each}
						</datalist>
					</label>
					<label class="check"><input type="checkbox" bind:checked={createNamespace} /> Create the namespace if missing</label>
				</div>
			{/if}
		</div>
		<div class="values-head">
			<span>Values</span>
			<span class="muted">
				{install ? 'Only what differs from the chart defaults is needed.' : 'These replace the values the release was given.'}
			</span>
		</div>
		<div class="editor">
			{#key valuesVersion}
				<YamlEditor bind:this={editor} value={values} readonly={false} onsave={() => preview()} />
			{/key}
		</div>
		<div class="footer">
			<button onclick={oncancel}>Cancel</button>
			<button class="primary" onclick={preview} disabled={!!busy}>{busy ?? 'Review'} <kbd>Ctrl+S</kbd></button>
		</div>
	{:else}
		<div class="review">
			<p>
				{install ? 'Installing' : 'Upgrading'} gives <strong class="mono">{review.outcome.name}</strong>
				revision {review.outcome.revision} with {chartLabel(review.outcome)}{review.outcome.appVersion
					? ` (app ${review.outcome.appVersion})`
					: ''}.
				{#if review.outcome.hooks}
					<span class="muted">The chart also runs {review.outcome.hooks} {review.outcome.hooks === 1 ? 'hook' : 'hooks'}.</span>
				{/if}
			</p>
			<HelmCompare
				namespace={review.outcome.namespace}
				before={{ values: initial?.values ?? {}, manifest: initial?.manifest ?? '' }}
				after={{ values: review.values, manifest: review.outcome.manifest }}
			/>
			{#if review.outcome.notes.trim()}
				<h4>Notes</h4>
				<pre class="notes">{review.outcome.notes.trim()}</pre>
			{/if}
		</div>
		<div class="footer">
			<button onclick={() => (review = null)} disabled={!!busy}>Back to editing <kbd>Esc</kbd></button>
			<button class="primary" onclick={run} disabled={!!busy || !!blocked} title={blocked ?? ''}>
				{busy ?? (install ? 'Install' : 'Upgrade')}
			</button>
		</div>
	{/if}
</div>

<style>
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
	.form {
		display: flex;
		flex-direction: column;
		gap: 10px;
		padding: 14px 16px;
		border-bottom: 1px solid var(--border);
	}
	.row {
		display: flex;
		flex-wrap: wrap;
		align-items: flex-end;
		gap: 10px;
	}
	.field {
		display: grid;
		gap: 4px;
		font-size: 12px;
		color: var(--text-muted);
		font-weight: 600;
	}
	.field.grow {
		flex: 1;
		min-width: 220px;
	}
	.field input {
		padding: 6px 8px;
		border: 1px solid var(--border);
		border-radius: 6px;
		background: var(--surface-1);
		color: var(--text);
		font-family: var(--mono);
		font-size: 12.5px;
		font-weight: 400;
	}
	.short {
		width: 110px;
	}
	.check {
		display: flex;
		align-items: center;
		gap: 6px;
		padding-bottom: 6px;
		font-size: 12.5px;
	}
	button {
		padding: 6px 12px;
		border: 1px solid var(--border);
		border-radius: 6px;
		background: var(--surface-1);
		font-size: 12.5px;
	}
	button.primary {
		border-color: var(--accent);
		background: var(--accent);
		color: var(--on-accent);
		font-weight: 600;
	}
	button:disabled {
		opacity: 0.55;
	}
	.segmented {
		align-self: flex-start;
	}
	.hint {
		margin: 0;
		font-size: 12.5px;
	}
	.found {
		display: grid;
		gap: 2px;
		max-height: 240px;
		margin: 0;
		padding: 0;
		overflow: auto;
		list-style: none;
	}
	.pick {
		display: grid;
		grid-template-columns: max-content max-content minmax(0, 1fr);
		gap: 10px;
		width: 100%;
		border: none;
		text-align: left;
	}
	.pick:hover {
		background: var(--surface-2);
	}
	.desc {
		overflow: hidden;
		color: var(--text-muted);
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.values-head {
		display: flex;
		gap: 10px;
		align-items: baseline;
		padding: 10px 16px 6px;
		font-size: 12px;
		font-weight: 650;
	}
	.values-head .muted {
		font-weight: 400;
	}
	.editor {
		display: flex;
		flex: 1;
		min-height: 220px;
	}
	.editor > :global(.editor) {
		flex: 1;
	}
	.review {
		flex: 1;
		min-height: 0;
		padding: 12px 16px;
		overflow: auto;
		font-size: 13px;
	}
	.review > p {
		margin: 0 0 6px;
	}
	h4 {
		margin: 16px 0 6px;
		font-size: 12px;
	}
	.notes {
		margin: 0;
		padding: 10px 12px;
		border: 1px solid var(--border-subtle);
		border-radius: 8px;
		font-family: var(--mono);
		font-size: 12px;
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}
	.footer {
		display: flex;
		justify-content: flex-end;
		gap: 8px;
		padding: 12px 16px;
		border-top: 1px solid var(--border);
	}
	.mono {
		font-family: var(--mono);
		font-size: 12.5px;
	}
	.muted {
		color: var(--text-muted);
	}
	kbd {
		margin-left: 4px;
		font-family: var(--mono);
		font-size: 10.5px;
		opacity: 0.8;
	}
</style>
