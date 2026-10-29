<script lang="ts">
	import { parse, stringify } from 'yaml';
	import { canEdit, save } from '../actions';
	import { backend, type EventInfo, type KubeObject, type Reference } from '../api';
	import { errorMessage } from '../format';
	import { drafts } from '../navigation.svelte';
	import { containerNames, hasLogs, hasUsers, isPods } from '../pods';
	import { app, type Details, type DetailsTab } from '../state.svelte';
	import { cellTone } from '../status';
	import { drill, drillLabel, objectActions, subjectOf } from '../objects';
	import ActionBar from './ActionBar.svelte';
	import Describe from './Describe.svelte';
	import EventTimeline from './EventTimeline.svelte';
	import Icon from './Icon.svelte';
	import LogView from './LogView.svelte';
	import PodSummary from './PodSummary.svelte';
	import TerminalView from './TerminalView.svelte';
	import YamlEditor from './YamlEditor.svelte';

	let { details }: { details: Details } = $props();

	const REFRESH = 5000;

	let object = $state<KubeObject | null>(null);
	let events = $state<EventInfo[]>([]);
	let error = $state<string | null>(null);
	let showManaged = $state(false);
	let saveError = $state<string | null>(null);
	let saving = $state(false);
	let now = $state(Date.now());
	let editor = $state<ReturnType<typeof YamlEditor>>();

	async function load() {
		try {
			const obj = await backend.getObject(details.target);
			object = obj;
			error = null;
			now = Date.now();
			if (obj.metadata.uid) {
				events = await backend.events(details.target.namespace, obj.metadata.uid).catch(() => events);
			}
		} catch (err) {
			error = errorMessage(err);
		}
	}

	$effect(() => {
		void details.target;
		object = null;
		events = [];
		load();
		const timer = setInterval(() => {
			if (!details.editing && ['describe', 'events', 'yaml'].includes(details.tab)) load();
		}, REFRESH);
		return () => clearInterval(timer);
	});

	/** What uses the object, as k9s `u` finds it, looked up when Describe shows; or why not. */
	let usedBy = $state<Reference[] | { error: string } | 'loading'>('loading');
	$effect(() => {
		const target = details.target;
		usedBy = 'loading';
		if (!hasUsers(details.resource) || details.tab !== 'describe') return;
		let current = true;
		backend
			.usedBy(target)
			.then((refs) => current && (usedBy = refs))
			.catch((err) => current && (usedBy = { error: errorMessage(err) }));
		return () => {
			current = false;
		};
	});

	// Logs and shells keep running in the background once opened, until another object is shown.
	let opened = $state({ logs: false, shell: false });
	/** Containers picked in Describe, for the logs and the shell to switch to. */
	let picked = $state<{ logs: { name: string; previous?: boolean } | null; shell: { name: string } | null }>({
		logs: null,
		shell: null
	});
	$effect(() => {
		void details.target;
		opened = { logs: false, shell: false };
		picked = { logs: null, shell: null };
	});

	function openContainer(name: string, tab: 'logs' | 'shell') {
		picked[tab] = { name };
		app.showDetails(details.resource, details.target, tab);
	}

	/** The logs of one container, of the instance that ended or the current one. */
	function openLogs(name: string, previous: boolean) {
		picked.logs = { name, previous };
		app.showDetails(details.resource, details.target, 'logs', { previous });
	}
	$effect(() => {
		if (details.tab === 'logs' || details.tab === 'shell') opened[details.tab] = true;
	});

	const logs = $derived(hasLogs(details.resource, object));
	// Shells change what runs in the cluster, so read-only leaves them out.
	const shell = $derived(isPods(details.resource) && !app.readOnly);
	const containers = $derived(containerNames(object));
	const targetKey = $derived(JSON.stringify(details.target));

	function close() {
		app.details = null;
		app.maximized = false;
	}

	/** Logs and shells, which take the place of the list unless kept beside it. */
	const tool = $derived(details.tab === 'logs' || details.tab === 'shell');

	const MIN_WIDTH = 320;
	let aside = $state<HTMLElement>();
	let width = $state(0);

	/** Sets the width beside the list, keeping 320px for each side. */
	function resize(width: number) {
		const room = aside?.parentElement?.clientWidth ?? window.innerWidth;
		app.panelWidth = Math.max(MIN_WIDTH, Math.min(room - MIN_WIDTH, width));
	}

	/** Drags the left edge of the panel. */
	function onpointerdown(e: PointerEvent) {
		if (e.button !== 0 || !aside) return;
		e.preventDefault();
		const handle = e.currentTarget as HTMLElement;
		handle.setPointerCapture(e.pointerId);
		const right = aside.getBoundingClientRect().right;
		const move = (ev: PointerEvent) => resize(right - ev.clientX);
		const up = () => {
			handle.removeEventListener('pointermove', move);
			handle.removeEventListener('pointerup', up);
			handle.removeEventListener('pointercancel', up);
		};
		handle.addEventListener('pointermove', move);
		handle.addEventListener('pointerup', up);
		handle.addEventListener('pointercancel', up);
	}

	function onresizekey(e: KeyboardEvent) {
		const step = e.shiftKey ? 80 : 20;
		if (e.key === 'ArrowLeft') resize(width + step);
		else if (e.key === 'ArrowRight') resize(width - step);
		else if (e.key === 'Home') app.panelWidth = null;
		else return;
		e.preventDefault();
		e.stopPropagation();
	}

	const yamlText = $derived.by(() => {
		if (!object) return '';
		const copy = structuredClone($state.snapshot(object)) as KubeObject;
		if (!showManaged) delete copy.metadata.managedFields;
		return stringify(copy, { lineWidth: 0 });
	});

	async function onsave(text: string): Promise<boolean> {
		if (saving) return false;
		let value: KubeObject;
		try {
			value = parse(text);
		} catch (err) {
			saveError = `Invalid YAML: ${errorMessage(err)}`;
			return false;
		}
		// Another name or namespace would save a different object; another resourceVersion would
		// overwrite changes made since the edit began.
		const meta = value && typeof value === 'object' ? value.metadata : undefined;
		if (
			meta?.name !== details.target.name ||
			meta?.namespace !== object?.metadata.namespace ||
			meta?.resourceVersion !== object?.metadata.resourceVersion
		) {
			saveError = 'Keep the name, namespace and resourceVersion as they are.';
			return false;
		}
		saving = true;
		drafts.saving = true;
		saveError = null;
		const ok = await save(details.target, value);
		saving = false;
		drafts.saving = false;
		if (ok) {
			drafts.dirty = false;
			details.editing = false;
			load();
		} else {
			saveError = 'Saving failed. Your draft is still here. Review the error before retrying.';
		}
		return ok;
	}

	/** Stops editing; with unsaved changes, asks first. */
	function cancel() {
		if (drafts.block(cancel)) return;
		details.editing = false;
		saveError = null;
	}

	function tab(value: DetailsTab) {
		app.showDetails(details.resource, details.target, value);
	}

	// What leaving the edit does: save the editor's text, or drop it.
	$effect(() => {
		drafts.save = () => (editor ? onsave(editor.text()) : Promise.resolve(false));
		drafts.discard = () => {
			details.editing = false;
			saveError = null;
		};
		return () => {
			drafts.save = null;
			drafts.discard = null;
			drafts.dirty = false;
		};
	});

	/** A pod's state in a word, as its list shows it: why a container waits, or else the phase. */
	const status = $derived.by(() => {
		if (object?.kind !== 'Pod') return null;
		const statuses: any[] = [
			...(object.status?.initContainerStatuses ?? []),
			...(object.status?.containerStatuses ?? [])
		];
		const reason = statuses.map((s) => s.state?.waiting?.reason ?? s.state?.terminated?.reason).find(Boolean);
		const text: string | undefined = object.metadata.deletionTimestamp ? 'Terminating' : (reason ?? object.status?.phase);
		return text ? { text, tone: cellTone('Status', text) } : null;
	});

	/** The tabs show these already. */
	const TABS = new Set(['describe', 'yaml', 'logs', 'shell', 'usedBy']);

	const actions = $derived(
		objectActions(subjectOf(details.resource, details.target, object), {
			show: (tab, extra) => app.showDetails(details.resource, details.target, tab, extra),
			enter: drillLabel(details.resource) ? () => void drill(details.resource, details.target) : undefined,
			namespace: app.namespace
		}).filter((a) => !a.id || !TABS.has(a.id))
	);

	const warnings = $derived(events.filter((e) => e.kind === 'Warning').length);
</script>

<aside aria-label="{details.resource.kind} inspector" class:compact={tool} bind:this={aside} bind:clientWidth={width}>
	{#if !app.panelWide}
		<!-- A focusable separator is a widget: it takes the arrow keys. -->
		<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
		<div
			class="resize"
			role="separator"
			aria-orientation="vertical"
			aria-label="Resize the inspector"
			aria-valuenow={width}
			aria-valuemin={MIN_WIDTH}
			tabindex="0"
			title="Drag to resize, double-click for the default width"
			{onpointerdown}
			ondblclick={() => (app.panelWidth = null)}
			onkeydown={onresizekey}
		></div>
	{/if}
	<header>
		<div class="title">
			<span class="kind">{details.resource.kind} inspector</span>
			<h2 title={details.target.name}>{details.target.name}</h2>
			<p class="context-path">{app.cluster?.context} / {details.target.namespace ?? 'cluster-scoped'}</p>
			{#if status}
				<p class="status {status.tone}">
					<Icon name={status.tone === 'ok' ? 'ok' : status.tone === 'bad' ? 'critical' : 'pending'} size={15} />
					{status.text}
				</p>
			{/if}
		</div>
		<div class="window">
			{#if tool && !app.maximized}
				<button
					class="icon beside"
					class:on={app.toolsBeside}
					onclick={() => (app.toolsBeside = !app.toolsBeside)}
					onmousedown={(e) => e.preventDefault()}
					title={app.toolsBeside ? 'Hide the list and give the whole width to the ' + details.tab : 'Show the list beside the ' + details.tab}
					aria-label="Show the list beside"
					aria-pressed={app.toolsBeside}><Icon name="resource" size={17} /></button
				>
			{/if}
			<button
				class="icon"
				onclick={() => (app.maximized = !app.maximized)}
				onmousedown={(e) => e.preventDefault()}
				title={app.maximized ? 'Restore (f)' : 'Maximize (f)'}
				aria-label={app.maximized ? 'Restore' : 'Maximize'}>{app.maximized ? '⤡' : '⤢'}</button
			>
			<button class="icon close" onclick={close} title="Close (Esc)" aria-label="Close"><Icon name="close" /></button>
		</div>
	</header>

	<nav aria-label="Inspector views">
		<button class:active={details.tab === 'describe'} onclick={() => tab('describe')}>Overview <kbd>d</kbd></button>
		<button class:active={details.tab === 'events'} onclick={() => tab('events')}>
			Events <span class="n" class:alert={warnings}>{events.length}</span>
		</button>
		<button class:active={details.tab === 'yaml'} onclick={() => tab('yaml')}>YAML <kbd>y</kbd></button>
		{#if logs}
			<button class:active={details.tab === 'logs'} onclick={() => tab('logs')}>Logs <kbd>l</kbd></button>
		{/if}
		{#if shell}
			<button class:active={details.tab === 'shell'} onclick={() => tab('shell')}>Shell <kbd>s</kbd></button>
		{/if}
		<span class="spacer"></span>
		{#if details.tab === 'yaml'}
			<label class="managed"><input type="checkbox" bind:checked={showManaged} disabled={details.editing} /> managed fields</label>
			{#if details.editing}
				<button class="secondary" onclick={() => cancel()}>Cancel</button>
				<button class="primary" disabled={saving} onclick={() => editor && onsave(editor.text())}>
					{saving ? 'Saving…' : 'Save'} <kbd>Ctrl+S</kbd>
				</button>
			{:else if canEdit(details.resource) && !app.readOnly}
				<button class="secondary" onclick={() => (details.editing = true)} disabled={!object}>Edit <kbd>e</kbd></button>
			{/if}
		{/if}
	</nav>

	{#if error}
		<div class="banner">{error}</div>
	{/if}
	{#if saveError}
		<div class="banner">{saveError}</div>
	{/if}

	<div class="body">
		{#key targetKey}
			{#if opened.logs && logs}
				<div class="tool" class:hidden={details.tab !== 'logs'}>
					<LogView
						target={details.target}
						{containers}
						previous={details.previous}
						container={picked.logs}
						active={details.tab === 'logs'}
					/>
				</div>
			{/if}
			{#if opened.shell && shell}
				<div class="tool" class:hidden={details.tab !== 'shell'}>
					<TerminalView target={details.target} {containers} mode={details.mode} container={picked.shell} />
				</div>
			{/if}
		{/key}
		{#if details.tab === 'logs' || details.tab === 'shell'}
			<!-- Shown above. -->
		{:else if !object}
			{#if !error}<p class="muted">Loading…</p>{/if}
		{:else if details.tab === 'describe'}
			<div class="scroll">
				<PodSummary {object} onlogs={logs ? openLogs : undefined} />
				<Describe
					{object}
					{events}
					{now}
					decoded={details.decoded ?? false}
					ondecode={() => (details.decoded = !details.decoded)}
					oncontainer={openContainer}
					onevents={() => tab('events')}
					{shell}
					usedBy={hasUsers(details.resource) ? usedBy : undefined}
				/>
			</div>
		{:else if details.tab === 'events'}
			<div class="scroll events">
				<EventTimeline {events} {now} />
			</div>
		{:else}
			{#key showManaged}
				<YamlEditor
					value={yamlText}
					readonly={!details.editing}
					{onsave}
					oncancel={cancel}
					onchange={(dirty) => (drafts.dirty = dirty)}
					bind:this={editor}
				/>
			{/key}
		{/if}
	</div>
	<ActionBar {actions} hints={false} compact />
</aside>

<style>
	aside {
		position: relative;
		display: flex;
		flex-direction: column;
		min-height: 0;
		min-width: 0;
		border-left: 1px solid var(--border);
		background: var(--surface-1);
	}
	header {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: 12px;
		padding: 18px 16px 12px 20px;
	}
	.resize {
		position: absolute;
		z-index: 2;
		top: 0;
		bottom: 0;
		left: -4px;
		width: 8px;
		cursor: col-resize;
		touch-action: none;
	}
	.resize:hover,
	.resize:focus-visible {
		outline: none;
		background: linear-gradient(to right, transparent 3px, var(--accent) 3px, var(--accent) 5px, transparent 5px);
	}
	/* Narrow windows give the inspector the whole width anyway. */
	@media (max-width: 1100px) {
		.resize,
		.beside {
			display: none;
		}
	}
	/* Logs and shells want the height: the title takes one line. */
	.compact header {
		align-items: center;
		padding: 10px 12px 8px 20px;
	}
	.compact .title {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		gap: 4px 12px;
	}
	.compact .kind {
		display: none;
	}
	.compact h2,
	.compact .context-path,
	.compact .status {
		margin: 0;
	}
	.icon.on {
		background: var(--surface-2);
		color: var(--accent-strong);
	}
	.context-path {
		margin: 4px 0 0;
		font-size: 12px;
		color: var(--text-muted);
		overflow-wrap: anywhere;
	}
	.status {
		display: flex;
		align-items: center;
		gap: 6px;
		margin: 8px 0 0;
		font-size: 12.5px;
		font-weight: 600;
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
	.n {
		margin-left: 2px;
		padding: 0 5px;
		border-radius: 4px;
		background: var(--surface-2);
		font-size: 11px;
		font-weight: 600;
	}
	.n.alert {
		background: color-mix(in srgb, var(--bad) 12%, var(--surface-1));
		color: var(--bad);
	}
	.scroll {
		overflow: auto;
	}
	.events {
		padding: 18px 20px;
	}
	.title {
		min-width: 0;
	}
	.kind {
		font-size: 11px;
		font-weight: 650;
		letter-spacing: 0.1em;
		text-transform: uppercase;
		color: var(--text-muted);
	}
	h2 {
		margin: 6px 0 0;
		font-size: 16px;
		font-family: var(--mono);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.window {
		display: flex;
		gap: 2px;
	}
	.icon {
		display: grid;
		place-items: center;
		width: 30px;
		height: 30px;
		padding: 0;
		border: none;
		border-radius: 6px;
		background: none;
		font-size: 16px;
		color: var(--text-muted);
	}
	.icon.close {
		font-size: 20px;
	}
	.icon:hover {
		background: var(--surface-2);
	}
	/* Narrow windows scroll the tabs rather than the page. */
	nav {
		display: flex;
		align-items: center;
		gap: 4px;
		padding: 0 16px 0 14px;
		border-bottom: 1px solid var(--border);
		overflow-x: auto;
		scrollbar-width: none;
	}
	nav > * {
		flex: none;
	}
	nav > button:not(.primary):not(.secondary) {
		padding: 8px 10px;
		border: none;
		border-bottom: 2px solid transparent;
		background: none;
		color: var(--text-muted);
	}
	nav > button.active {
		color: var(--accent-strong);
		border-bottom-color: var(--accent);
		font-weight: 600;
	}
	.spacer {
		flex: 1;
	}
	.managed {
		display: flex;
		align-items: center;
		gap: 4px;
		margin-right: 8px;
		font-size: 12px;
		color: var(--text-muted);
	}
	.primary,
	.secondary {
		margin: 4px 0;
		padding: 4px 10px;
		border-radius: 6px;
		font-size: 13px;
	}
	.primary {
		border: 1px solid var(--accent);
		background: var(--accent);
		color: var(--on-accent);
	}
	.secondary {
		border: 1px solid var(--border);
		background: var(--surface-1);
	}
	kbd {
		margin-left: 4px;
		font-family: var(--mono);
		font-size: 10px;
		opacity: 0.7;
	}
	.banner {
		margin: 10px 16px 0;
		padding: 8px 12px;
		border-radius: 8px;
		background: color-mix(in srgb, var(--bad) 15%, transparent);
		color: var(--bad);
		font-size: 13px;
	}
	.body {
		flex: 1;
		min-height: 0;
		display: flex;
		flex-direction: column;
	}
	.body > :global(*) {
		flex: 1;
		min-height: 0;
	}
	.tool {
		display: flex;
		flex-direction: column;
	}
	.tool > :global(*) {
		flex: 1;
	}
	.tool.hidden {
		display: none;
	}
	.muted {
		padding: 20px;
		color: var(--text-muted);
	}
</style>
