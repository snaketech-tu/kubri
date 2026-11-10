<script lang="ts">
	import { lightDismiss } from '../dismiss';
	import { untrack } from 'svelte';
	import { backend, type DebugRequest } from '../api';
	import { errorMessage } from '../format';
	import { app, findResource, type Dialog } from '../state.svelte';
	import { t } from '../i18n';
	import Phrase from './Phrase.svelte';
	import { plainOptions } from '../select';
	import Select from './Select.svelte';

	let { dialog }: { dialog: Extract<Dialog, { kind: 'debug' }> } = $props();
	// Read once: the dialog closes by clearing it.
	const d = untrack(() => dialog);
	const context = untrack(() => app.cluster?.context);

	let box = $state<HTMLDialogElement>();
	let way = $state<'ephemeral' | 'copy'>(d.way ?? 'ephemeral');
	/** The pod's containers; debug containers aim at one of them. */
	let containers = $state<string[]>([]);
	let container = $state(d.container ?? '');
	let image = $state(untrack(() => app.prefs.settings.debug.image) || 'busybox');
	let namespace = $state('default');
	let busy = $state(false);
	let error = $state<string | null>(null);

	$effect(() => {
		if (d.on !== 'pod') return;
		backend.getObject(d.target).then(
			(pod) => {
				const spec = (pod.spec ?? {}) as { containers?: { name: string }[] };
				containers = (spec.containers ?? []).map((c) => c.name);
				container ||= containers[0] ?? '';
			},
			(err) => (error = errorMessage(err))
		);
	});

	$effect(() => {
		const previous = document.activeElement as HTMLElement | null;
		box?.showModal();
		return () => previous?.focus();
	});

	function close() {
		if (!busy) app.dialog = null;
	}

	function request(): DebugRequest {
		if (d.on === 'node') return { kind: 'node', node: d.target.name, namespace: namespace.trim() || null, image };
		if (way === 'copy') return { kind: 'copy', target: d.target, container: container || null };
		return { kind: 'ephemeral', target: d.target, container: container || null, image };
	}

	async function start(e: SubmitEvent) {
		e.preventDefault();
		if (app.cluster?.context !== context) {
			error = t('The context changed since this dialog opened. Close it and start again.');
			return;
		}
		busy = true;
		error = null;
		try {
			const session = await backend.debug(request());
			if (d.on === 'node' || way === 'ephemeral') app.prefs.settings.debug.image = image.trim();
			const pods = findResource(app.resources, { group: '', version: 'v1', plural: 'pods' });
			app.dialog = null;
			if (pods) app.showDetails(pods, session.pod, 'shell', { mode: session.mode, container: session.container });
			if (session.note) app.toast(session.note);
		} catch (err) {
			error = errorMessage(err);
		} finally {
			busy = false;
		}
	}
</script>

<dialog
	{@attach lightDismiss}
	class="dialog"
	aria-label={d.on === 'node' ? t('Node shell') : t('Debug')}
	bind:this={box}
	oncancel={(e) => {
		e.preventDefault();
		close();
	}}
>
	<form onsubmit={start}>
		{#if d.on === 'node'}
			<h2>{t('Shell on node {name}', { name: d.target.name })}</h2>
			<div class="warning" role="note">
				<strong>{t('This shell has full control of the node.')}</strong>
				{t('Kubri starts a privileged pod on the node that shares its processes, network and files, and opens a shell in them. Anything you do there acts on the node itself. The pod is removed when the shell ends.')}
			</div>
			<label>
				<span>{t('Image')}</span>
				<input bind:value={image} required spellcheck="false" />
			</label>
			<p class="hint"><Phrase text={t('Needs {sh} and {nsenter}, which busybox has.')} code={{ sh: 'sh', nsenter: 'nsenter' }} /></p>
			<label>
				<span>{t('Namespace')}</span>
				<input bind:value={namespace} spellcheck="false" />
			</label>
			<p class="hint">{t('Where the pod runs. It needs a namespace that allows privileged pods.')}</p>
		{:else}
			<h2>{t('Debug {name}', { name: d.target.name })}</h2>
			<div class="ways" role="radiogroup" aria-label={t('How to debug')}>
				<label class:active={way === 'ephemeral'}>
					<input type="radio" name="way" value="ephemeral" bind:group={way} />
					<strong>{t('Debug container')}</strong>
					<span>{t('A container with your tools joins the pod and sees the processes of the container you pick. Works for images without a shell, such as distroless ones.')}</span>
				</label>
				<label class:active={way === 'copy'}>
					<input type="radio" name="way" value="copy" bind:group={way} />
					<strong>{t('Debug copy')}</strong>
					<span>{t('A copy of the pod in which the container sleeps instead of running its command, for crash loops. The copy gets no labels, so it takes no traffic, and stays until you delete it.')}</span>
				</label>
			</div>
			{#if containers.length > 1}
				<label>
					<span>{t('Container')}</span>
					<Select bind:value={container} options={plainOptions(containers)} mono />
				</label>
			{/if}
			{#if way === 'ephemeral'}
				<label>
					<span>{t('Image')}</span>
					<input bind:value={image} required spellcheck="false" />
				</label>
				<p class="hint">{t('Kubri remembers the image you use. Debug containers stay in the pod until it is replaced.')}</p>
			{/if}
		{/if}
		{#if error}<p class="error" role="alert">{error}</p>{/if}
		<div class="buttons">
			<button type="button" class="secondary" onclick={close} disabled={busy}>{t('Cancel')}</button>
			<button type="submit" class="primary" disabled={busy}>
				{busy ? t('Starting…') : d.on === 'node' ? t('Open node shell') : way === 'copy' ? t('Create copy') : t('Start debug container')}
			</button>
		</div>
	</form>
</dialog>

<style>
	.dialog {
		position: fixed;
		inset: 0;
		z-index: 51;
		width: min(560px, calc(100vw - 32px));
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
		margin: 0 0 14px;
		font-size: 17px;
		overflow-wrap: anywhere;
	}
	form > label {
		display: grid;
		grid-template-columns: 90px 1fr;
		align-items: center;
		gap: 10px;
		margin-top: 12px;
		font-size: 13px;
	}
	input:not([type='radio']) {
		min-width: 0;
		padding: 6px 8px;
		border: 1px solid var(--border);
		border-radius: 6px;
		background: var(--surface-1);
		color: var(--text);
		font-family: var(--mono);
		font-size: 13px;
	}
	.hint {
		margin: 4px 0 0 100px;
		color: var(--text-muted);
		font-size: 12px;
	}
	.ways {
		display: grid;
		gap: 8px;
	}
	.ways label {
		display: grid;
		grid-template-columns: auto 1fr;
		gap: 2px 10px;
		padding: 10px 12px;
		border: 1px solid var(--border);
		border-radius: 8px;
		cursor: pointer;
	}
	.ways label.active {
		border-color: var(--accent);
		background: color-mix(in srgb, var(--accent) 6%, var(--surface-1));
	}
	.ways input {
		grid-row: span 2;
		margin: 3px 0 0;
		accent-color: var(--accent);
		box-shadow: none;
	}
	.ways span {
		color: var(--text-muted);
		font-size: 12.5px;
	}
	.warning {
		padding: 10px 12px;
		border: 1px solid color-mix(in srgb, var(--bad) 40%, transparent);
		border-radius: 8px;
		background: color-mix(in srgb, var(--bad) 8%, transparent);
		font-size: 13px;
	}
	.error {
		margin: 12px 0 0;
		padding: 10px 12px;
		border-radius: 8px;
		background: color-mix(in srgb, var(--bad) 10%, transparent);
		color: var(--bad);
		overflow-wrap: anywhere;
	}
	.hint :global(code) {
		font-family: var(--mono);
	}
	.buttons {
		display: flex;
		justify-content: flex-end;
		gap: 8px;
		margin-top: 18px;
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
	@media (max-width: 640px) {
		form > label {
			grid-template-columns: 1fr;
		}
		.hint {
			margin-left: 0;
		}
	}
</style>
