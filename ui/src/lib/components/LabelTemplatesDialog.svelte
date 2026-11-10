<script lang="ts">
	import { onMount } from 'svelte';
	import type { LabelTemplate } from '../api';
	import { lightDismiss } from '../dismiss';
	import { t } from '../i18n';
	import { app } from '../state.svelte';

	let { onclose }: { onclose: (picked?: string) => void } = $props();

	const FIELDS = [
		['namespace', 'namespace', 'exported_namespace'],
		['pod', 'pod', 'k8s_pod_name'],
		['container', 'container', 'k8s_container_name']
	] as const;

	let templates = $state<LabelTemplate[]>(app.prefs.settings.labelTemplates.map((x) => ({ ...x })));
	/** The names the templates had when the dialog opened, to follow a rename. */
	const originals = new Map<LabelTemplate, string>();
	let saving = $state(false);
	let box = $state<HTMLDialogElement>();

	// Once: removing the last template must not add an empty one again.
	onMount(() => {
		for (const x of templates) originals.set(x, x.name);
		box?.showModal();
		if (!templates.length) add();
	});

	function add() {
		templates.push({ name: '', namespace: '', pod: '', container: '' });
		queueMicrotask(() => box?.querySelectorAll<HTMLInputElement>('input[data-name]')[templates.length - 1]?.focus());
	}

	async function save(e: SubmitEvent) {
		e.preventDefault();
		saving = true;
		// Rows left without a name or a label are dropped, not refused.
		const kept = templates.filter((x) => x.name.trim() || x.namespace.trim() || x.pod.trim() || x.container.trim());
		const ok = await app.prefs.setLabelTemplates(kept);
		// Contexts that picked a renamed template keep it under its new name.
		for (const x of ok ? kept : []) {
			const before = originals.get(x);
			const after = x.name.trim();
			if (!before || before === after) continue;
			for (const [context, settings] of Object.entries(app.prefs.settings.contexts)) {
				if (settings.integrations?.labels === before) {
					await app.prefs.setContextSettings(context, { ...settings, integrations: { ...settings.integrations, labels: after } });
				}
			}
		}
		saving = false;
		if (ok) onclose(kept.length === 1 ? kept[0].name.trim() : undefined);
	}
</script>

<dialog
	bind:this={box}
	{@attach lightDismiss}
	class="dialog"
	aria-label={t('Label templates')}
	oncancel={(e) => {
		e.preventDefault();
		onclose();
	}}
>
	<form onsubmit={save}>
		<h2>{t('Label templates')}</h2>
		<p class="muted">
			{t('Some log collectors give logs other label names, such as exported_namespace. A template names them once; each context picks one on its integrations page. Leave a field empty for the usual name.')}
		</p>
		{#each templates as template, i (i)}
			<fieldset>
				<legend class="sr-only">{template.name || t('New template')}</legend>
				<label class="name">
					<span>{t('Name')}</span>
					<input data-name bind:value={template.name} placeholder={t('such as OpenTelemetry')} spellcheck="false" autocomplete="off" />
				</label>
				{#each FIELDS as [key, usual, example] (key)}
					<label>
						<span class="mono">{usual}</span>
						<input class="mono" bind:value={template[key]} placeholder={example} spellcheck="false" autocomplete="off" />
					</label>
				{/each}
				<button type="button" class="remove" onclick={() => templates.splice(i, 1)} aria-label={t('Remove {name}', { name: template.name || t('New template') })}>
					{t('Remove')}
				</button>
			</fieldset>
		{/each}
		<button type="button" class="add" onclick={add}>{t('Add template')}</button>
		<div class="buttons">
			<button type="button" class="secondary" onclick={() => onclose()}>{t('Cancel')}</button>
			<button type="submit" class="primary" disabled={saving}>{saving ? t('Saving…') : t('Save')}</button>
		</div>
	</form>
</dialog>

<style>
	.dialog {
		position: fixed;
		inset: 0;
		z-index: 51;
		width: min(760px, calc(100vw - 32px));
		max-height: 85dvh;
		margin: auto;
		overflow: auto;
		padding: 20px 22px;
		border: 1px solid var(--border);
		border-radius: 12px;
		background: var(--surface-1);
		box-shadow: var(--shadow);
		color: var(--text);
	}
	.dialog::backdrop {
		background: rgb(16 21 34 / 45%);
	}
	h2 {
		margin: 0 0 8px;
		font-size: 17px;
	}
	.muted {
		margin: 0 0 12px;
		color: var(--text-muted);
		font-size: 13px;
	}
	fieldset {
		display: grid;
		grid-template-columns: minmax(0, 0.8fr) repeat(3, minmax(0, 1.2fr)) auto;
		gap: 8px;
		align-items: end;
		margin: 0 0 10px;
		padding: 10px 0 0;
		border: none;
		border-top: 1px solid var(--border);
	}
	@media (max-width: 560px) {
		fieldset {
			grid-template-columns: 1fr 1fr;
		}
		.name {
			grid-column: 1 / -1;
		}
	}
	label {
		display: flex;
		flex-direction: column;
		gap: 4px;
		min-width: 0;
		font-size: 12px;
		color: var(--text-muted);
	}
	input {
		width: 100%;
		min-width: 0;
		box-sizing: border-box;
	}
	.mono {
		font-family: var(--mono);
	}
	.remove,
	.add {
		padding: 6px 10px;
		border: 1px solid var(--border);
		border-radius: 6px;
		background: var(--surface-1);
		color: var(--text);
		font-size: 12.5px;
	}
	.add {
		margin-top: 2px;
	}
	.buttons {
		display: flex;
		justify-content: flex-end;
		gap: 8px;
		margin-top: 16px;
	}
	.buttons button {
		padding: 7px 14px;
		border-radius: 7px;
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
	.sr-only {
		position: absolute;
		width: 1px;
		height: 1px;
		overflow: hidden;
		clip: rect(0 0 0 0);
	}
</style>
