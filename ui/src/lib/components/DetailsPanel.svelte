<script lang="ts">
	import { parse, stringify } from 'yaml';
	import { canEdit, save } from '../actions';
	import { backend, type EventInfo, type KubeObject } from '../api';
	import { errorMessage } from '../format';
	import { app, type Details } from '../state.svelte';
	import Describe from './Describe.svelte';
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
			if (!details.editing) load();
		}, REFRESH);
		return () => clearInterval(timer);
	});

	const yamlText = $derived.by(() => {
		if (!object) return '';
		const copy = structuredClone($state.snapshot(object)) as KubeObject;
		if (!showManaged) delete copy.metadata.managedFields;
		return stringify(copy, { lineWidth: 0 });
	});

	async function onsave(text: string) {
		let value: KubeObject;
		try {
			value = parse(text);
		} catch (err) {
			saveError = `Invalid YAML: ${errorMessage(err)}`;
			return;
		}
		saving = true;
		saveError = null;
		const ok = await save(details.target, value);
		saving = false;
		if (ok) {
			details.editing = false;
			load();
		} else {
			saveError = 'Saving failed. If the object changed in the meantime, cancel and edit again.';
		}
	}

	function cancel() {
		details.editing = false;
		saveError = null;
	}

	const title = $derived(
		details.target.namespace ? `${details.target.namespace}/${details.target.name}` : details.target.name
	);
</script>

<aside>
	<header>
		<div class="title">
			<span class="kind">{details.resource.kind}</span>
			<h2 title={title}>{title}</h2>
		</div>
		<button class="close" onclick={() => (app.details = null)} title="Close (Esc)">×</button>
	</header>

	<nav>
		<button class:active={details.tab === 'describe'} onclick={() => (details.tab = 'describe')}>Describe <kbd>d</kbd></button>
		<button class:active={details.tab === 'yaml'} onclick={() => (details.tab = 'yaml')}>YAML <kbd>y</kbd></button>
		<span class="spacer"></span>
		{#if details.tab === 'yaml'}
			<label class="managed"><input type="checkbox" bind:checked={showManaged} disabled={details.editing} /> managed fields</label>
			{#if details.editing}
				<button class="secondary" onclick={cancel}>Cancel</button>
				<button class="primary" disabled={saving} onclick={() => editor && onsave(editor.text())}>
					{saving ? 'Saving…' : 'Save'} <kbd>Ctrl+S</kbd>
				</button>
			{:else if canEdit(details.resource)}
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
		{#if !object}
			{#if !error}<p class="muted">Loading…</p>{/if}
		{:else if details.tab === 'describe'}
			<Describe {object} {events} {now} />
		{:else}
			{#key showManaged}
				<YamlEditor
					value={yamlText}
					readonly={!details.editing}
					{onsave}
					oncancel={cancel}
					bind:this={editor}
				/>
			{/key}
		{/if}
	</div>
</aside>

<style>
	aside {
		display: flex;
		flex-direction: column;
		min-height: 0;
		min-width: 0;
		border-left: 1px solid var(--border);
		background: var(--surface-1);
	}
	header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
		padding: 14px 16px 8px 20px;
	}
	.title {
		min-width: 0;
	}
	.kind {
		font-size: 11px;
		font-weight: 600;
		letter-spacing: 0.06em;
		text-transform: uppercase;
		color: var(--accent-strong);
	}
	h2 {
		margin: 2px 0 0;
		font-size: 16px;
		font-family: var(--mono);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.close {
		width: 30px;
		height: 30px;
		border: none;
		border-radius: 6px;
		background: none;
		font-size: 20px;
		color: var(--text-muted);
	}
	.close:hover {
		background: var(--surface-2);
	}
	nav {
		display: flex;
		align-items: center;
		gap: 4px;
		padding: 0 16px 0 14px;
		border-bottom: 1px solid var(--border);
	}
	nav > button:not(.primary):not(.secondary) {
		padding: 8px 10px;
		border: none;
		border-bottom: 2px solid transparent;
		background: none;
		color: var(--text-muted);
	}
	nav > button.active {
		color: var(--text);
		border-bottom-color: var(--accent);
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
		color: white;
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
	.muted {
		padding: 20px;
		color: var(--text-muted);
	}
</style>
