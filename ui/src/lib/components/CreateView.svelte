<script lang="ts">
	import { untrack } from 'svelte';
	import { backend, type ApplyItem, type ApplyOutcome, type ResourceInfo } from '../api';
	import { CORE_GROUPS, parseObjects, TEMPLATE_KINDS, toYaml, workloadOf } from '../create';
	import { FORMS } from '../forms';
	import { creating } from '../create.svelte';
	import { errorMessage } from '../format';
	import { app, findResource } from '../state.svelte';
	import DiffView from './DiffView.svelte';
	import FormPanel from './FormPanel.svelte';
	import Icon from './Icon.svelte';
	import PageHeader from './PageHeader.svelte';
	import YamlEditor from './YamlEditor.svelte';

	let editor = $state<YamlEditor>();
	/** For objects that name no namespace. */
	let namespace = $state(untrack(() => creating.namespace || app.namespace) || 'default');
	/** The context to apply in. */
	let into = $state(untrack(() => creating.into ?? app.cluster?.context ?? ''));
	$effect(() => {
		creating.namespace = namespace;
		creating.into = into === app.cluster?.context ? null : into;
	});
	const elsewhere = $derived(!!into && into !== app.cluster?.context);
	const production = $derived(into ? app.contextSettingsOf(into).production : app.production);
	let busy = $state(false);
	let error = $state<string | null>(null);
	/** The dry run, or once applied, what happened. */
	let results = $state<ApplyItem[] | null>(null);
	let applied = $state(false);
	/** The context the dry run ran in; applying elsewhere is refused. */
	let reviewedIn = $state<string | null>(null);
	let objects: unknown[] = [];
	let urlOpen = $state(false);
	let url = $state('');
	let file = $state<HTMLInputElement>();
	let kind = $state('');

	/** Kinds you can create: the common ones first, then the rest by name. */
	const creatable = $derived(app.resources.filter((r) => r.verbs.includes('create') && !r.plural.includes('/')));
	const common = $derived(
		TEMPLATE_KINDS.map((k) => creatable.find((r) => r.kind === k && CORE_GROUPS.includes(r.group))).filter(
			(r): r is ResourceInfo => !!r
		)
	);
	const others = $derived(
		creatable.filter((r) => !common.includes(r)).sort((a, b) => a.kind.localeCompare(b.kind) || a.group.localeCompare(b.group))
	);

	const key = (r: ResourceInfo) => `${r.group}/${r.version}/${r.plural}`;

	/** Adds the kind's template, as a new document when there is YAML already. */
	async function insert() {
		const resource = creatable.find((r) => key(r) === kind);
		kind = '';
		if (!resource) return;
		const before = editor?.text() ?? creating.text;
		// A Service or Ingress added after a workload points at it.
		const related = ['Service', 'Ingress'].includes(resource.kind) ? workloadOf(parseObjects(before).objects) : {};
		await creating.start(resource, { ...related, namespace: resource.namespaced && namespace ? namespace : undefined });
		if (before.trim()) creating.set(`${before.trimEnd()}\n---\n${creating.text}`);
	}

	function load(text: string) {
		const before = editor?.text() ?? creating.text;
		creating.set(before.trim() ? `${before.trimEnd()}\n---\n${text.trim()}\n` : text);
	}

	async function openFile(e: Event) {
		const input = e.currentTarget as HTMLInputElement;
		const chosen = [...(input.files ?? [])];
		input.value = '';
		try {
			load((await Promise.all(chosen.map((f) => f.text()))).map((t) => t.trim()).join('\n---\n'));
		} catch (err) {
			error = errorMessage(err);
		}
	}

	async function fromUrl(e: SubmitEvent) {
		e.preventDefault();
		busy = true;
		error = null;
		try {
			load(await backend.fetchManifest(url.trim()));
			urlOpen = false;
			url = '';
		} catch (err) {
			error = errorMessage(err);
		} finally {
			busy = false;
		}
	}

	async function paste() {
		try {
			load(await navigator.clipboard.readText());
		} catch {
			error = 'Kubri may not read the clipboard here. Paste into the editor with Ctrl+V instead.';
		}
	}

	/** Runs the YAML as a server-side dry run and shows what would change. */
	async function review(text = editor?.text() ?? creating.text) {
		creating.text = text;
		const parsed = parseObjects(text);
		error = parsed.error;
		if (parsed.error) return;
		if (!parsed.objects.length) {
			error = 'There is nothing to apply yet: start from a kind, or paste YAML.';
			return;
		}
		objects = parsed.objects;
		reviewedIn = app.cluster?.context ?? null;
		busy = true;
		try {
			results = await backend.apply({ objects, namespace, dryRun: true, context: reviewedIn, into });
			applied = false;
		} catch (err) {
			error = errorMessage(err);
		} finally {
			busy = false;
		}
	}

	async function apply() {
		busy = true;
		error = null;
		try {
			results = await backend.apply({ objects, namespace, dryRun: false, context: reviewedIn, into });
			applied = true;
			// Kinds a new CustomResourceDefinition serves join the sidebar and the palette.
			if (!elsewhere && results.some((r) => r.kind === 'CustomResourceDefinition' && (r.outcome === 'created' || r.outcome === 'changed'))) {
				app.resources = await backend.resources();
			}
			const done = results.filter((r) => r.outcome === 'created' || r.outcome === 'changed').length;
			const failed = results.filter((r) => r.outcome === 'failed').length;
			app.toast(
				failed
					? `${failed} of ${results.length} could not be applied`
					: done
						? `Applied ${done} ${done === 1 ? 'object' : 'objects'}`
						: 'Nothing changed',
				failed ? 'bad' : 'ok'
			);
		} catch (err) {
			error = errorMessage(err);
		} finally {
			busy = false;
		}
	}

	/** Keeps the draft as typed, for when you come back to the page. */
	function sync() {
		error = null;
		if (editor) creating.text = editor.text();
	}

	/** The one object in the editor, when there is a form for its kind; read once typing pauses. */
	let formObject = $state<Record<string, any> | null>(null);
	let showForm = $state(true);
	$effect(() => {
		const text = creating.text;
		const timer = setTimeout(() => {
			const { objects } = parseObjects(text);
			const only = objects.length === 1 ? (objects[0] as Record<string, any>) : null;
			formObject = only && typeof only === 'object' && FORMS[only.kind] ? only : null;
		}, 250);
		return () => clearTimeout(timer);
	});

	function back() {
		results = null;
		applied = false;
		error = null;
	}

	function startOver() {
		back();
		creating.set('');
		creating.source = null;
	}

	const failed = $derived(results?.filter((r) => r.outcome === 'failed').length ?? 0);
	const changes = $derived(results?.filter((r) => r.outcome === 'created' || r.outcome === 'changed' || r.outcome === 'unchecked').length ?? 0);
	const blocked = $derived(
		app.readOnly
			? 'Kubri is read-only for this context'
			: failed
				? 'Fix what failed in the dry run first'
				: !changes
					? 'Nothing would change'
					: null
	);

	const LABELS: Record<ApplyOutcome, string> = {
		created: 'Create',
		changed: 'Change',
		unchanged: 'Unchanged',
		unchecked: 'Create, not checked',
		failed: 'Fails'
	};
	const DONE: Record<ApplyOutcome, string> = {
		created: 'Created',
		changed: 'Changed',
		unchanged: 'Unchanged',
		unchecked: 'Not checked',
		failed: 'Failed'
	};

	function open(item: ApplyItem) {
		const target = item.target;
		const resource = target && findResource(app.resources, target.resource);
		if (target && resource) app.showDetails(resource, target);
	}

	function onkeydown(e: KeyboardEvent) {
		if (app.overlayOpen || !results || e.key !== 'Escape') return;
		e.preventDefault();
		back();
	}
</script>

<svelte:window {onkeydown} />

<section>
	<PageHeader
		crumbs={['Create']}
		title="Create"
		subtitle="Objects from a template, a file, a URL or the clipboard, applied like kubectl apply --server-side after a dry run"
	/>

	<div class="card">
		{#if !results}
			<div class="toolbar">
				<label class="field">
					<span>Add</span>
					<select bind:value={kind} aria-label="Add a kind's template">
						<option value="" disabled>Choose a kind…</option>
						<optgroup label="Common">
							{#each common as r (key(r))}<option value={key(r)}>{r.kind}</option>{/each}
						</optgroup>
						<optgroup label="All kinds">
							{#each others as r (key(r))}<option value={key(r)}>{r.kind}{r.group ? ` (${r.group})` : ''}</option>{/each}
						</optgroup>
					</select>
				</label>
				<button onclick={insert} disabled={!kind} title="Add the kind's template, as another document if there is YAML already">Add</button>
				<span class="divider"></span>
				<button onclick={() => file?.click()}><Icon name="config" size={15} /> Open files…</button>
				<input bind:this={file} type="file" accept=".yaml,.yml,.json" multiple hidden onchange={openFile} />
				<button onclick={() => (urlOpen = !urlOpen)} aria-expanded={urlOpen}><Icon name="ingress" size={15} /> From URL…</button>
				<button onclick={paste}><Icon name="download" size={15} /> Paste</button>
				<span class="spacer"></span>
				{#if formObject}
					<button class:on={showForm} aria-pressed={showForm} onclick={() => (showForm = !showForm)} title="A form for the {formObject.kind}"
						>Form</button
					>
				{/if}
				<label class="field">
					<span>Context</span>
					<select bind:value={into} title="The cluster to create the objects in" aria-label="Create in context">
						{#each app.contexts as c (c.name)}<option value={c.name}>{app.contextName(c.name)}</option>{/each}
					</select>
				</label>
				<label class="field">
					<span>Namespace</span>
					<input
						class="namespace"
						bind:value={namespace}
						list="create-namespaces"
						spellcheck="false"
						title="For objects that name no namespace"
						aria-label="Namespace for objects that name none"
					/>
					<datalist id="create-namespaces">
						{#each app.namespaces as n (n)}<option value={n}></option>{/each}
					</datalist>
				</label>
			</div>
			{#if creating.source}
				<p class="source">
					<Icon name="config" size={14} /> A copy of <strong>{creating.source}</strong> without what the cluster assigned. Pick
					another namespace or context, or change the name.
				</p>
			{/if}
			{#if urlOpen}
				<form class="url" onsubmit={fromUrl}>
					<!-- svelte-ignore a11y_autofocus -->
					<input bind:value={url} type="url" required autofocus placeholder="https://… a YAML file, such as a GitHub page of one" spellcheck="false" />
					<button class="primary" type="submit" disabled={busy}>{busy ? 'Reading…' : 'Load'}</button>
				</form>
			{/if}
			<div class="work">
				<div class="editor">
					{#key creating.version}
						<YamlEditor bind:this={editor} value={creating.text} readonly={false} onsave={(text) => review(text)} onchange={sync} />
					{/key}
					{#if !creating.text.trim()}
						<p class="empty">
							Add a kind's template, open files, load a URL or paste YAML. Several objects separated by <code>---</code> are applied
							in dependency order: namespaces and config before the workloads that use them.
						</p>
					{/if}
				</div>
				{#if formObject && showForm}
					<FormPanel object={formObject} onchange={(o) => creating.set(toYaml([o]))} />
				{/if}
			</div>
		{:else}
			<div class="toolbar">
				<strong>{applied ? 'Applied' : 'Dry run'}</strong>
				<span class="muted">
					{#if applied}
						in {app.contextName(into)}
					{:else}
						{results.length} {results.length === 1 ? 'object' : 'objects'} checked by the API server in {app.contextName(into)}; nothing has changed yet
					{/if}
				</span>
			</div>
			<ol class="results">
				{#each results as item (item.index)}
					<li class={item.outcome}>
						<details open={!applied && results.length <= 3 && item.outcome !== 'unchanged'}>
							<summary>
								<span class="outcome">{applied ? DONE[item.outcome] : LABELS[item.outcome]}</span>
								<span class="kind">{item.kind}</span>
								<span class="name">{item.namespace ? `${item.namespace}/` : ''}{item.name || '(no name)'}</span>
								{#if item.message}<span class="message">{item.message}</span>{/if}
								{#if applied && item.target && item.outcome !== 'failed' && !elsewhere}
									<button class="link" onclick={(e) => (e.preventDefault(), open(item))}>Open</button>
								{/if}
							</summary>
							{#if item.after}
								<DiffView before={item.before} after={item.after} maxHeight="420px" />
							{/if}
						</details>
					</li>
				{/each}
			</ol>
		{/if}
		{#if error}<p class="error" role="alert">{error}</p>{/if}
		<div class="buttons">
			{#if !results}
				{#if production}<span class="note"><Icon name="warning" size={14} /> {app.contextName(into)} is a production context</span>{/if}
				<button class="secondary" onclick={startOver} disabled={busy || !creating.text}>Clear</button>
				<button class="primary" onclick={() => review()} disabled={busy}>{busy ? 'Checking…' : 'Review'} <kbd>Ctrl+S</kbd></button>
			{:else if applied}
				<button class="secondary" onclick={startOver}>Start over</button>
				<button class="primary" onclick={back}>Back to the YAML</button>
			{:else}
				{#if blocked}<span class="note">{blocked}</span>{/if}
				<button class="secondary" onclick={back} disabled={busy}>Back to editing <kbd>Esc</kbd></button>
				<button class="primary" onclick={apply} disabled={busy || !!blocked}>
					{busy ? 'Applying…' : changes ? `Apply ${changes} ${changes === 1 ? 'change' : 'changes'}` : 'Apply'}
				</button>
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
		padding: 0 24px 20px;
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
	.toolbar {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 8px;
		padding: 12px 16px;
		border-bottom: 1px solid var(--border);
		font-size: 13px;
	}
	.toolbar > button {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		padding: 6px 10px;
		border: 1px solid var(--border);
		border-radius: 7px;
		background: var(--surface-1);
		font-size: 12.5px;
	}
	.field {
		display: inline-flex;
		align-items: center;
		gap: 8px;
		color: var(--text-muted);
	}
	.field select {
		max-width: 260px;
		padding: 5px 8px;
		color: var(--text);
		font-size: 12.5px;
	}
	.spacer {
		flex: 1;
	}
	.divider {
		width: 1px;
		height: 20px;
		margin: 0 4px;
		background: var(--border);
	}
	.toolbar > button:disabled {
		opacity: 0.5;
	}
	.muted {
		color: var(--text-muted);
	}
	.field .namespace {
		width: 160px;
		padding: 5px 8px;
		font-family: var(--mono);
		font-size: 12.5px;
		color: var(--text);
	}
	.source {
		display: flex;
		align-items: center;
		gap: 8px;
		margin: 0;
		padding: 8px 16px;
		border-bottom: 1px solid var(--border);
		background: color-mix(in srgb, var(--accent) 6%, var(--surface-1));
		font-size: 12.5px;
	}
	.url {
		display: flex;
		gap: 8px;
		padding: 10px 16px;
		border-bottom: 1px solid var(--border);
	}
	.url input {
		flex: 1;
		padding: 6px 10px;
		font-family: var(--mono);
		font-size: 12.5px;
	}
	.work {
		display: flex;
		flex: 1;
		min-height: 0;
	}
	.editor {
		position: relative;
		flex: 1;
		min-width: 0;
		min-height: 0;
	}
	.toolbar > button.on {
		border-color: var(--accent);
		color: var(--accent);
	}
	@media (max-width: 900px) {
		.work {
			flex-direction: column;
		}
	}
	.editor :global(.cm-editor) {
		height: 100%;
	}
	.empty {
		position: absolute;
		top: 40%;
		left: 50%;
		width: min(520px, 80%);
		margin: 0;
		transform: translate(-50%, -50%);
		color: var(--text-muted);
		text-align: center;
		line-height: 1.6;
		pointer-events: none;
	}
	.results {
		flex: 1;
		min-height: 0;
		margin: 0;
		padding: 8px 16px;
		overflow: auto;
		list-style: none;
	}
	.results li {
		border-bottom: 1px solid var(--border-subtle);
	}
	summary {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		gap: 4px 10px;
		padding: 10px 0;
		cursor: pointer;
		font-size: 13px;
	}
	.outcome {
		min-width: 92px;
		font-weight: 600;
	}
	.created .outcome {
		color: var(--ok);
	}
	.changed .outcome {
		color: var(--accent-strong, var(--accent));
	}
	.unchanged .outcome,
	.unchecked .outcome {
		color: var(--text-muted);
	}
	.failed .outcome,
	.failed .message {
		color: var(--bad);
	}
	.kind {
		color: var(--text-muted);
	}
	.name {
		font-family: var(--mono);
		font-size: 12.5px;
	}
	.message {
		flex-basis: 100%;
		padding-left: 102px;
		color: var(--text-muted);
		font-size: 12.5px;
		overflow-wrap: anywhere;
	}
	details[open] > :global(:not(summary)) {
		margin-bottom: 12px;
	}
	.link {
		margin-left: auto;
		padding: 0;
		border: none;
		background: none;
		color: var(--accent);
		font-size: 12.5px;
	}
	.error {
		margin: 0;
		padding: 10px 16px;
		background: color-mix(in srgb, var(--bad) 10%, transparent);
		color: var(--bad);
		font-size: 13px;
		overflow-wrap: anywhere;
	}
	.buttons {
		display: flex;
		align-items: center;
		justify-content: flex-end;
		gap: 8px;
		padding: 12px 16px;
		border-top: 1px solid var(--border);
	}
	.buttons button {
		display: inline-flex;
		align-items: center;
		gap: 8px;
		padding: 8px 14px;
		border-radius: 8px;
	}
	.note {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		margin-right: auto;
		color: var(--text-muted);
		font-size: 12.5px;
	}
	.primary {
		border: 1px solid var(--accent);
		background: var(--accent);
		color: var(--on-accent);
	}
	.primary:disabled {
		opacity: 0.6;
	}
	.secondary {
		border: 1px solid var(--border);
		background: var(--surface-1);
	}
	kbd {
		font-size: 11px;
		opacity: 0.8;
	}
	code {
		font-family: var(--mono);
	}
	@media (max-width: 720px) {
		section {
			padding: 0 12px 12px;
		}
		.message {
			padding-left: 0;
		}
	}
</style>
