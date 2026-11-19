<script lang="ts">
	import { tick, untrack } from 'svelte';
	import { lightDismiss } from '../dismiss';
	import { backend, type FavoriteForward, type KubeObject, type ObjectRef } from '../api';
	import { errorMessage } from '../format';
	import {
		assignPorts,
		entriesOf,
		fewAtATime,
		forwardKind,
		parseImport,
		refOfKind,
		remotePortOf,
		type ForwardEntry,
		type ImportProblem,
		type PortConflict
	} from '../forward-import';
	import { plural, t } from '../i18n';
	import { coreText } from '../i18n/errors';
	import { plainOptions } from '../select';
	import { ALL_NAMESPACES, app, sameResource, type Dialog } from '../state.svelte';
	import Phrase from './Phrase.svelte';
	import Select from './Select.svelte';

	let { dialog }: { dialog: Extract<Dialog, { kind: 'addFavorites' }> } = $props();

	let box = $state<HTMLDialogElement>();
	let picker = $state<HTMLInputElement>();
	/** The context the dialog opened in; favorites are kept for it only. */
	const context = untrack(() => app.cluster?.context);
	const defaultNamespace = untrack(() => app.cluster?.defaultNamespace || 'default');

	let step = $state<'source' | 'plan'>('source');
	let text = $state('');
	let source = $state('');
	let dragging = $state(false);
	let skipped = $state<ImportProblem[]>([]);

	let namespace = $state(
		untrack(() => dialog.namespace ?? (app.namespace !== ALL_NAMESPACES ? app.namespace : defaultNamespace))
	);
	const namespaces = $derived([...new Set([namespace, ...app.namespaces])].filter(Boolean).sort());
	let labels = $state('');

	/** A favorite to keep, as checked against the cluster. */
	interface Row {
		key: string;
		entry: ForwardEntry;
		target: ObjectRef | null;
		remote: number | null;
		/** Why it cannot be kept as it is: no such object or port. */
		problem: string | null;
		/** A favorite with this target and port is kept already. */
		kept: boolean;
		/** The local port, as given, typed or chosen. */
		local: number | null;
		chosen: boolean;
		conflict: PortConflict | null;
		keep: boolean;
	}
	let rows = $state<Row[]>([]);
	let startOnConnect = $state(false);
	let startNow = $state(true);
	let busy = $state<string | null>(null);
	let error = $state<string | null>(null);
	/** Ports found in use, by listen address, so planning again does not ask for them twice. */
	const inUse = new Map<string, Set<number>>();

	$effect(() => {
		const previous = document.activeElement as HTMLElement | null;
		box?.showModal();
		return () => previous?.focus();
	});

	// Opened on chosen objects or a namespace: straight to the plan.
	$effect(() => {
		untrack(() => {
			if (dialog.targets?.length) void fromObjects(dialog.targets);
			else if (dialog.namespace) void fromNamespace();
		});
	});

	let saving = $state(false);

	function close() {
		if (!saving) app.dialog = null;
	}

	const sameContext = () => context && context === app.cluster?.context && !app.connecting;

	async function load(targets: ObjectRef[]): Promise<Map<string, KubeObject | string>> {
		const found = new Map<string, KubeObject | string>();
		await fewAtATime(targets, async (target) => {
			try {
				found.set(objectKey(target), await backend.getObject({ ...target, context }));
			} catch (err) {
				found.set(objectKey(target), errorMessage(err));
			}
		});
		return found;
	}

	const objectKey = (target: ObjectRef) => `${target.resource.group}/${target.resource.plural}/${target.namespace ?? ''}/${target.name}`;

	async function fromObjects(targets: ObjectRef[]) {
		busy = t('Checking…');
		error = null;
		const objects = await load(targets);
		const entries = entriesOf(
			targets.flatMap((target) => {
				const object = objects.get(objectKey(target));
				const kind = forwardKind(target.resource.plural);
				return object && typeof object !== 'string' && kind ? [{ kind, object }] : [];
			})
		);
		busy = null;
		if (!entries.length) {
			error =
				targets.length === 1
					? t('{name} declares no TCP port. Start a port-forward to it and keep it as a favorite instead.', { name: targets[0].name })
					: t('None of them declares a TCP port.');
			return;
		}
		await plan(entries, objects);
	}

	async function fromNamespace() {
		const services = refOfKind(app.resources, 'services');
		if (!services) return;
		busy = t('Checking…');
		error = null;
		try {
			const ns = namespace;
			const result = await backend.search({ text: '', kinds: [services], allKinds: false, namespace: ns, labels: labels.trim(), fields: [], contexts: [], limit: 2000 });
			if (ns !== namespace) return;
			if (!result.hits.length) {
				busy = null;
				error = result.problems.length
					? coreText(result.problems[0].message)
					: labels.trim()
						? t('No Service in {namespace} has the labels {labels}.', { namespace: ns, labels: labels.trim() })
						: t('{namespace} has no Services.', { namespace: ns });
				return;
			}
			await fromObjects(result.hits.map((h) => h.object));
		} catch (err) {
			busy = null;
			error = errorMessage(err);
		}
	}

	async function fromText() {
		const result = parseImport(text);
		error = null;
		if (result.format === 'none') {
			error = t('There are no kubectl port-forward commands or favorites in this text.');
			return;
		}
		skipped = result.problems;
		if (!result.entries.length) {
			error = t('None of it could be read. See below why.');
			return;
		}
		await plan(result.entries);
	}

	async function read(file: File | undefined) {
		if (!file) return;
		error = null;
		if (file.size > 1024 * 1024) {
			error = t('{name} is too large for a list of port-forwards.', { name: file.name });
			return;
		}
		text = await file.text();
		source = file.name;
	}

	/** Checks the entries against the cluster, leaves out ones listed twice, and gives each a local port. */
	async function plan(entries: ForwardEntry[], loaded?: Map<string, KubeObject | string>) {
		busy = t('Checking…');
		error = null;
		const kept = app.prefs.contextSettingsOf(context ?? '').forwards ?? [];
		const made = entries.map((entry): Row => {
			const resource = refOfKind(app.resources, entry.kind);
			const target = resource ? { resource, namespace: entry.namespace ?? defaultNamespace, name: entry.name } : null;
			return {
				key: '',
				entry,
				target,
				remote: typeof entry.remote === 'number' ? entry.remote : null,
				problem: resource ? null : t('The cluster does not serve {kind}', { kind: entry.kind }),
				kept: false,
				local: entry.local,
				chosen: false,
				conflict: null,
				keep: true
			};
		});
		const objects = loaded ?? (await load(made.flatMap((r) => (r.target ? [r.target] : []))));
		const seen = new Set<string>();
		const out: Row[] = [];
		for (const row of made) {
			if (row.target && !row.problem) {
				const object = objects.get(objectKey(row.target));
				if (typeof object === 'string' || !object) {
					row.problem = object ?? t('not found');
				} else {
					const port = remotePortOf(row.entry, object);
					if (typeof port === 'string') row.problem = port;
					else row.remote = port;
				}
			}
			row.key = `${row.target ? objectKey(row.target) : `${row.entry.kind}/${row.entry.name}`}:${row.remote ?? row.entry.remote}`;
			if (seen.has(row.key)) continue;
			seen.add(row.key);
			row.kept = !!row.target && kept.some((f) => sameObject(f.target, row.target!) && f.remotePort === row.remote);
			row.keep = !row.problem && !row.kept;
			out.push(row);
		}
		if (!sameContext()) {
			busy = null;
			error = t('The context changed. Close this dialog and review the targets again.');
			return;
		}
		rows = out;
		step = 'plan';
		await (settling = settlePorts());
		busy = null;
		await tick();
		box?.querySelector<HTMLElement>('.plan input')?.focus();
	}

	function sameObject(a: ObjectRef, b: ObjectRef) {
		return sameResource(a.resource, b.resource) && (a.namespace ?? null) === (b.namespace ?? null) && a.name === b.name;
	}

	/** Rows that get a port: those that can be kept. */
	const open = (r: Row) => !r.problem && !r.kept;

	/**
	 * Gives every row a local port: the one given or typed, else a free one. Ports something
	 * listens on are asked for until the chosen ones are all free.
	 */
	async function settlePorts() {
		const favorites = (app.prefs.contextSettingsOf(context ?? '').forwards ?? []).map((f) => f.localPort);
		const planned = rows.filter(open);
		for (let round = 0; round < 6; round++) {
			const busyPorts = planned.flatMap((r) => [...(inUse.get(r.entry.address) ?? [])]);
			const ports = assignPorts(
				planned.map((r) => ({ remote: r.remote!, local: r.chosen ? null : r.local })),
				favorites,
				busyPorts
			);
			planned.forEach((r, i) => {
				r.local = ports[i].local;
				r.chosen = ports[i].chosen;
				r.conflict = ports[i].conflict;
			});
			// Ask about the ports not asked about yet, by listen address.
			const fresh = new Map<string, number[]>();
			for (const r of planned) {
				const known = inUse.get(r.entry.address) ?? new Set();
				if (r.local !== null && !known.has(r.local)) fresh.set(r.entry.address, [...(fresh.get(r.entry.address) ?? []), r.local]);
			}
			let found = false;
			for (const [address, list] of fresh) {
				const known = inUse.get(address) ?? new Set<number>();
				inUse.set(address, known);
				let taken: number[] = [];
				try {
					taken = await backend.portsInUse(address || null, list);
				} catch {
					// Not knowing is no reason to stop: the port is checked again when it starts.
				}
				// A forward Kubri runs to the same place is this favorite, not something in its way.
				const own = (port: number) =>
					app.forwards.some(
						(f) =>
							f.context === context &&
							f.localPort === port &&
							planned.some((r) => r.local === port && r.remote === f.remotePort && !!r.target && sameObject(r.target, f.target))
					);
				for (const port of taken) {
					if (own(port)) continue;
					known.add(port);
					found = true;
				}
			}
			if (!found) break;
		}
		rows = [...rows];
	}

	/** The last check of the ports, which keeping waits for. */
	let settling: Promise<void> = Promise.resolve();

	/** A typed port stays as it is typed; it is checked once the field is left. */
	function typed(row: Row, value: string) {
		const n = Number(value);
		row.local = value.trim() && Number.isInteger(n) && n >= 1 && n <= 65535 ? n : null;
		row.chosen = false;
	}

	/** Checks the ports again; an empty or wrong port lets Kubri choose one. */
	function changed(row: Row) {
		if (row.local === null) row.chosen = true;
		settling = settlePorts();
	}

	const keeping = $derived(rows.filter((r) => r.keep && open(r)));
	/** Two favorites on one port cannot run at once: that is fixed first. */
	const blocking = $derived(keeping.filter((r) => r.conflict === 'twice' || r.conflict === 'favorite' || r.local === null));
	const all = $derived(rows.filter(open));

	function conflictText(c: PortConflict): string {
		switch (c) {
			case 'twice':
				return t('Another line uses this port too');
			case 'favorite':
				return t('A favorite uses this port already');
			case 'inUse':
				return t('Something listens on this port now');
		}
	}

	async function keepAll() {
		// A port typed and then Keep clicked is checked first.
		await settling;
		if (busy || !keeping.length || blocking.length) return;
		if (!sameContext()) {
			error = t('The context changed. Close this dialog and review the targets again.');
			return;
		}
		busy = t('Saving…');
		saving = true;
		const favorites: FavoriteForward[] = keeping.map((r) => ({
			target: r.target!,
			name: r.entry.label,
			remotePort: r.remote!,
			localPort: r.local!,
			address: r.entry.address,
			startOnConnect: r.entry.startOnConnect ?? startOnConnect
		}));
		const saved = await app.favorites.add(favorites);
		busy = null;
		saving = false;
		if (!saved) return;
		app.dialog = null;
		app.toast(plural(favorites.length, 'Kept {n} port-forward favorite', 'Kept {n} port-forward favorites'));
		if (startNow) await app.favorites.switchAll(favorites, true);
	}

	function label(r: Row) {
		return `${r.target?.resource.plural ?? r.entry.kind}/${r.entry.name}`;
	}
</script>

<dialog
	{@attach lightDismiss}
	class="dialog"
	class:wide={step === 'plan'}
	aria-label={t('Add port-forward favorites')}
	aria-busy={!!busy}
	bind:this={box}
	oncancel={(e) => {
		e.preventDefault();
		close();
	}}
>
	<h2>{t('Add port-forward favorites')}</h2>
	{#if step === 'source'}
		<p class="lead">
			{t('Paste kubectl port-forward commands, a script with them, or a favorites file. Kubri checks each target and gives it a free local port before it keeps them.')}
		</p>
		<div
			class="drop"
			class:dragging
			role="region"
			aria-label={t('Drop a file here')}
			ondragover={(e) => {
				e.preventDefault();
				dragging = true;
			}}
			ondragleave={() => (dragging = false)}
			ondrop={(e) => {
				e.preventDefault();
				dragging = false;
				read(e.dataTransfer?.files[0]);
			}}
		>
			<span>{t('Drop a script or favorites file here, or')}</span>
			<button type="button" class="btn" onclick={() => picker?.click()}>{t('Choose file…')}</button>
			<input bind:this={picker} type="file" hidden onchange={(e) => read(e.currentTarget.files?.[0])} />
		</div>
		<label class="paste">
			<span>{source ? t('Or paste it (from {source})', { source }) : t('Or paste it')}</span>
			<textarea
				bind:value={text}
				rows="6"
				spellcheck="false"
				placeholder={'kubectl -n shop port-forward svc/api 8081:80 &\nkubectl -n shop port-forward deploy/web 3000'}
				oninput={() => (source = '')}
			></textarea>
		</label>
		{#if skipped.length}
			<details class="skipped" open>
				<summary>{plural(skipped.length, '{n} line was left out', '{n} lines were left out')}</summary>
				<ul>
					{#each skipped as p (`${p.at}:${p.text}`)}
						<li><span class="mono">{p.at}: {p.text}</span> <span class="why">{p.reason}</span></li>
					{/each}
				</ul>
			</details>
		{/if}

		<h3>{t('Or every Service in a namespace')}</h3>
		<div class="namespace">
			<label class="field">
				{t('Namespace')}
				<Select bind:value={namespace} options={plainOptions(namespaces)} searchable mono aria-label={t('Namespace')} />
			</label>
			<label class="field">
				{t('With labels (optional)')}
				<input bind:value={labels} placeholder="tier=backend" spellcheck="false" class="mono" />
			</label>
			<button type="button" class="btn" onclick={fromNamespace} disabled={!!busy || !namespace}>{t('Take its Services')}</button>
		</div>
		{#if error}<p class="error" role="alert">{error}</p>{/if}
		<div class="buttons">
			<button type="button" class="secondary" onclick={close}>{t('Cancel')}</button>
			<button type="button" class="primary" onclick={fromText} disabled={!!busy || !text.trim()}>{busy ?? t('Next')}</button>
		</div>
	{:else}
		<p class="lead">
			<Phrase
				text={t('For {context}. Kubri chose the local ports that are not given: the same as the target where it is free, else the target plus 10000. Change any of them here.')}
				strong={{ context: context ? app.prefs.contextName(context) : '' }}
			/>
		</p>
		{#if skipped.length}
			<p class="note">{plural(skipped.length, '{n} line was left out', '{n} lines were left out')}</p>
		{/if}
		<div class="plan" role="group" aria-label={t('Port-forwards to keep')}>
			<label class="row head">
				<input
					type="checkbox"
					checked={all.length > 0 && all.every((r) => r.keep)}
					indeterminate={all.some((r) => r.keep) && !all.every((r) => r.keep)}
					disabled={!all.length}
					onchange={(e) => {
						const on = e.currentTarget.checked;
						for (const r of all) r.keep = on;
					}}
				/>
				<span>{t('Target')}</span>
				<span class="port">{t('Port')}</span>
				<span class="local">{t('Local port')}</span>
			</label>
			{#each rows as row (row.key)}
				{@const usable = open(row)}
				<div class="row" class:off={!usable || !row.keep}>
					<input type="checkbox" bind:checked={row.keep} disabled={!usable} aria-label={t('Keep {name}', { name: label(row) })} />
					<div class="what">
						{#if row.entry.label}<strong>{row.entry.label}</strong>{/if}
						<span class="mono">{label(row)}</span>
						<small>
							{row.target?.namespace ?? row.entry.namespace ?? defaultNamespace}{row.entry.context && row.entry.context !== context
								? ` · ${t('from context {name}', { name: row.entry.context })}`
								: ''}
						</small>
						{#if row.problem}
							<small class="problem">{row.problem}</small>
						{:else if row.kept}
							<small>{t('A favorite already')}</small>
						{:else if row.conflict}
							<small class={row.conflict === 'inUse' ? 'warn' : 'problem'}>{conflictText(row.conflict)}</small>
						{/if}
					</div>
					<span class="port mono">{row.remote ?? row.entry.remote}</span>
					<span class="local">
						{#if usable}
							<input
								type="number"
								min="1"
								max="65535"
								class="mono"
								class:chosen={row.chosen}
								value={row.local ?? ''}
								aria-label={t('Local port for {name}', { name: label(row) })}
								title={row.chosen ? t('Chosen by Kubri') : ''}
								oninput={(e) => typed(row, e.currentTarget.value)}
								onchange={() => changed(row)}
							/>
						{/if}
					</span>
				</div>
			{/each}
		</div>
		{#if !all.length}
			<p class="note">{t('Nothing new to keep: each is a favorite already or cannot be forwarded to.')}</p>
		{/if}
		<div class="options">
			<label class="check">
				<input type="checkbox" bind:checked={startNow} />
				{t('Start them now')}
			</label>
			<label class="check" title={t('Start them whenever Kubri connects to this context')}>
				<input type="checkbox" bind:checked={startOnConnect} />
				{t('Start on connect')}
			</label>
		</div>
		{#if error}<p class="error" role="alert">{error}</p>{/if}
		<div class="buttons">
			{#if !dialog.targets?.length && !dialog.namespace}
				<button type="button" class="secondary back" onclick={() => (step = 'source')} disabled={!!busy}>{t('Back')}</button>
			{/if}
			<button type="button" class="secondary" onclick={close}>{t('Cancel')}</button>
			<button
				type="button"
				class="primary"
				onclick={keepAll}
				disabled={!!busy || !keeping.length || blocking.length > 0}
				title={blocking.length ? t('Give each a port of its own first') : ''}
			>
				{busy ?? plural(keeping.length, 'Keep {n} favorite', 'Keep {n} favorites')}
			</button>
		</div>
	{/if}
	{#if busy && step === 'source'}<p class="status" role="status">{busy}</p>{/if}
</dialog>

<style>
	.dialog {
		position: fixed;
		inset: 0;
		z-index: 51;
		width: min(620px, calc(100vw - 32px));
		max-height: 88dvh;
		margin: auto;
		overflow: auto;
		padding: 20px 22px;
		border: 1px solid var(--border);
		border-radius: 12px;
		background: var(--surface-1);
		box-shadow: var(--shadow);
		color: var(--text);
	}
	.dialog.wide {
		width: min(760px, calc(100vw - 32px));
	}
	.dialog::backdrop {
		background: rgb(16 21 34 / 45%);
	}
	h2 {
		margin: 0 0 6px;
		font-size: 17px;
	}
	h3 {
		margin: 18px 0 8px;
		font-size: 13px;
	}
	.lead {
		margin: 0 0 14px;
		color: var(--text-muted);
		font-size: 13px;
	}
	.lead :global(strong) {
		color: var(--text);
	}
	.drop {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		justify-content: center;
		gap: 10px;
		padding: 16px;
		border: 1.5px dashed var(--border);
		border-radius: 10px;
		color: var(--text-muted);
		font-size: 13px;
	}
	.drop.dragging {
		border-color: var(--accent);
		background: color-mix(in srgb, var(--accent) 6%, var(--surface-1));
	}
	.paste {
		display: grid;
		gap: 5px;
		margin-top: 12px;
		font-size: 12px;
		color: var(--text-muted);
	}
	textarea {
		padding: 8px;
		border: 1px solid var(--border);
		border-radius: 6px;
		background: var(--surface-1);
		color: var(--text);
		font-family: var(--mono);
		font-size: 12px;
		resize: vertical;
	}
	.mono {
		font-family: var(--mono);
	}
	.skipped {
		margin-top: 10px;
		font-size: 12px;
	}
	.skipped summary {
		color: var(--warn);
		cursor: pointer;
	}
	.skipped ul {
		margin: 6px 0 0;
		padding-left: 18px;
	}
	.skipped li {
		overflow-wrap: anywhere;
	}
	.why {
		color: var(--text-muted);
	}
	.namespace {
		display: grid;
		grid-template-columns: minmax(0, 1fr) minmax(0, 1fr) auto;
		align-items: end;
		gap: 10px;
	}
	@media (max-width: 560px) {
		.namespace {
			grid-template-columns: 1fr;
		}
	}
	.field {
		display: grid;
		gap: 5px;
		font-size: 12px;
		color: var(--text-muted);
	}
	.field input {
		padding: 6px 8px;
		border: 1px solid var(--border);
		border-radius: 6px;
		background: var(--surface-1);
		color: var(--text);
		font-size: 13px;
	}
	.plan + .note {
		margin-top: 10px;
	}
	.note {
		margin: 0 0 8px;
		color: var(--warn);
		font-size: 12px;
	}
	.plan {
		display: grid;
		border: 1px solid var(--border);
		border-radius: 8px;
		max-height: 46dvh;
		overflow: auto;
	}
	.row {
		display: grid;
		grid-template-columns: auto minmax(0, 1fr) 80px 110px;
		align-items: center;
		gap: 10px;
		padding: 7px 10px;
		border-top: 1px solid var(--border-subtle);
		font-size: 13px;
	}
	.row.head {
		position: sticky;
		top: 0;
		z-index: 1;
		border-top: none;
		background: var(--surface-2, var(--surface-1));
		color: var(--text-muted);
		font-size: 12px;
		font-weight: 600;
	}
	.row.off .what,
	.row.off .port {
		opacity: 0.6;
	}
	.row input[type='checkbox'],
	.check input {
		accent-color: var(--accent);
	}
	.what {
		display: grid;
		gap: 1px;
		min-width: 0;
	}
	.what > * {
		overflow-wrap: anywhere;
	}
	small {
		color: var(--text-muted);
		font-size: 12px;
	}
	.problem {
		color: var(--bad);
	}
	.warn {
		color: var(--warn);
	}
	.port {
		text-align: right;
	}
	.local input {
		width: 100%;
		padding: 4px 6px;
		border: 1px solid var(--border);
		border-radius: 6px;
		background: var(--surface-1);
		color: var(--text);
		font-size: 13px;
	}
	.local input.chosen {
		color: var(--text-muted);
	}
	@media (max-width: 560px) {
		.row {
			grid-template-columns: auto minmax(0, 1fr) 90px;
		}
		.row .port {
			display: none;
		}
	}
	.options {
		display: flex;
		flex-wrap: wrap;
		gap: 8px 18px;
		margin-top: 12px;
		font-size: 13px;
	}
	.check {
		display: inline-flex;
		align-items: center;
		gap: 6px;
	}
	.error {
		margin: 12px 0 0;
		padding: 10px 12px;
		border-radius: 8px;
		background: color-mix(in srgb, var(--bad) 10%, transparent);
		color: var(--bad);
		overflow-wrap: anywhere;
	}
	.status {
		margin: 10px 0 0;
		color: var(--text-muted);
		font-size: 12px;
	}
	.buttons {
		display: flex;
		justify-content: flex-end;
		gap: 8px;
		margin-top: 16px;
	}
	.buttons .back {
		margin-right: auto;
	}
	.buttons button {
		padding: 8px 14px;
		border-radius: 8px;
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
</style>
