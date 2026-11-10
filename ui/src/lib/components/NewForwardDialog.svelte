<script lang="ts">
	import { lightDismiss } from '../dismiss';
	import { untrack } from 'svelte';
	import { startPortForward } from '../actions';
	import { backend, type ObjectRef } from '../api';
	import { errorMessage } from '../format';
	import { forwardKinds, targetKey, targetOptions } from '../forwards';
	import { t } from '../i18n';
	import { coreText } from '../i18n/errors';
	import { forwardablePorts, startingPort, type ForwardablePort } from '../pods';
	import { plainOptions, type SelectOption } from '../select';
	import { ALL_NAMESPACES, app, refOf } from '../state.svelte';
	import PortForwardFields from './PortForwardFields.svelte';
	import Select from './Select.svelte';

	let box = $state<HTMLDialogElement>();
	/** The context the dialog opened in; it does not start a forward in another. */
	const context = untrack(() => app.cluster?.context);
	const kinds = untrack(() => forwardKinds(app.resources));

	let namespace = $state(
		untrack(() => (app.namespace !== ALL_NAMESPACES ? app.namespace : app.cluster?.defaultNamespace || 'default'))
	);
	const namespaces = $derived([...new Set([namespace, ...app.namespaces])].filter(Boolean).sort());

	/** The objects of the namespace; null while they load. */
	let options = $state.raw<SelectOption<string>[] | null>(null);
	let picked = $state('');
	let listError = $state<string | null>(null);

	let ports = $state<ForwardablePort[] | null>(null);
	let remotePort = $state<number | null>(null);
	let localPort = $state<number | null>(null);
	// k9s `portForwardAddress`, or K9S_DEFAULT_PF_ADDRESS.
	let address = $state(untrack(() => app.k9s?.portForwardAddress) ?? '127.0.0.1');
	let keep = $state(false);
	let busy = $state(false);
	let error = $state<string | null>(null);

	const target = $derived.by((): ObjectRef | null => {
		const kind = kinds.find((k) => picked.startsWith(`${k.group}/${k.plural}/`));
		if (!kind) return null;
		return { resource: refOf(kind), namespace, name: picked.slice(kind.group.length + kind.plural.length + 2) };
	});
	const resource = $derived(target && kinds.find((k) => k.plural === target.resource.plural && k.group === target.resource.group));

	$effect(() => {
		const previous = document.activeElement as HTMLElement | null;
		// The namespace comes first, and takes the focus.
		box?.showModal();
		return () => previous?.focus();
	});

	// The objects to forward to, again for each namespace.
	$effect(() => {
		const ns = namespace;
		// Another namespace holds other objects: choose again there.
		picked = '';
		options = null;
		listError = null;
		backend
			.search({ text: '', kinds: kinds.map(refOf), allKinds: false, namespace: ns, labels: '', fields: [], contexts: [], limit: 2000 })
			.then((result) => {
				if (ns !== namespace) return;
				options = targetOptions(result.hits, kinds);
				if (!options.length && result.problems.length) listError = coreText(result.problems[0].message);
			})
			.catch((err) => {
				if (ns !== namespace) return;
				options = [];
				listError = errorMessage(err);
			});
	});

	// The ports of the chosen object, starting with the one it asks for.
	$effect(() => {
		const chosen = target;
		ports = null;
		remotePort = null;
		localPort = null;
		error = null;
		if (!chosen) return;
		backend
			.getObject(chosen)
			.then((obj) => {
				if (target !== chosen) return;
				ports = forwardablePorts(obj);
				const start = startingPort(obj);
				if (start) ({ remote: remotePort, local: localPort } = start);
			})
			.catch((err) => {
				if (target !== chosen) return;
				ports = [];
				error = errorMessage(err);
			});
	});

	function close() {
		if (busy) return;
		app.dialog = null;
	}

	async function start(e?: SubmitEvent) {
		e?.preventDefault();
		// Enter pressed twice must not start a second forward.
		if (busy || !target) return;
		if (context !== app.cluster?.context || app.connecting) {
			error = t('The context changed. Close this dialog and review the targets again.');
			return;
		}
		if (!remotePort || remotePort < 1 || remotePort > 65535) {
			error = t('Pick a port between 1 and 65535');
			return;
		}
		if (localPort !== null && (localPort < 0 || localPort > 65535)) {
			error = t('Invalid local port');
			return;
		}
		busy = true;
		error = null;
		const ok = await startPortForward({ target, remotePort, localPort: localPort || null, address: address.trim() || null }, keep);
		busy = false;
		if (ok) app.dialog = null;
	}

	const placeholder = $derived(
		options === null ? t('Loading…') : options.length ? t('Choose a deployment, service or pod') : t('Nothing to forward to in this namespace')
	);
</script>

<dialog
	{@attach lightDismiss}
	class="dialog"
	aria-label={t('Start a port-forward')}
	bind:this={box}
	oncancel={(e) => {
		e.preventDefault();
		close();
	}}
>
	<form onsubmit={start}>
		<h2>{t('Start a port-forward')}</h2>
		<p class="muted">
			<strong>{context ? app.prefs.contextName(context) : ''}</strong>{#if target && resource}{' · '}{resource.kind}
				<span class="mono">{namespace}/{target.name}</span>{/if}
		</p>

		<div class="pick">
			<label class="field">
				{t('Namespace')}
				<Select bind:value={namespace} options={plainOptions(namespaces)} searchable mono aria-label={t('Namespace')} />
			</label>
			<label class="field">
				{t('Target')}
				<Select
					bind:value={picked}
					options={options ?? []}
					{placeholder}
					disabled={!options?.length}
					searchable
					mono
					aria-label={t('Target')}
				/>
			</label>
		</div>
		{#if listError}<p class="error" role="alert">{listError}</p>{/if}

		{#if target && resource}
			<div class="ports">
				<PortForwardFields
					{resource}
					{target}
					{ports}
					intro={false}
					bind:remotePort
					bind:localPort
					bind:address
					bind:keep
				/>
			</div>
		{/if}
		{#if error}<p class="error" role="alert">{error}</p>{/if}

		<div class="buttons">
			<button type="button" class="secondary" disabled={busy} onclick={close}>{t('Cancel')}</button>
			<button type="submit" class="primary" disabled={busy || !target || !remotePort}>{busy ? t('Working…') : t('Start')}</button>
		</div>
	</form>
</dialog>

<style>
	.dialog {
		position: fixed;
		inset: 0;
		z-index: 51;
		width: min(520px, calc(100vw - 32px));
		max-height: 85dvh;
		margin: auto;
		overflow: auto;
		padding: 20px 22px;
		border: 1px solid var(--border);
		border-radius: 12px;
		background: var(--surface-1);
		box-shadow: var(--shadow);
		color: var(--text);
		outline: none;
	}
	.dialog::backdrop {
		background: rgb(16 21 34 / 45%);
	}
	h2 {
		margin: 0 0 6px;
		font-size: 17px;
	}
	p {
		margin: 0 0 10px;
	}
	.muted {
		color: var(--text-muted);
		overflow-wrap: anywhere;
	}
	.mono {
		font-family: var(--mono);
		font-size: 12.5px;
	}
	.pick {
		display: grid;
		grid-template-columns: minmax(0, 1fr) minmax(0, 1.4fr);
		gap: 10px;
	}
	.field {
		display: flex;
		flex-direction: column;
		gap: 4px;
		min-width: 0;
		margin: 10px 0;
		font-size: 13px;
		color: var(--text-muted);
	}
	.ports {
		margin-top: 6px;
		padding-top: 12px;
		border-top: 1px solid var(--border);
	}
	.error {
		color: var(--bad);
	}
	.buttons {
		display: flex;
		justify-content: flex-end;
		gap: 8px;
		margin-top: 18px;
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
	@media (max-width: 520px) {
		.mono {
		font-family: var(--mono);
		font-size: 12.5px;
	}
	.pick {
			grid-template-columns: 1fr;
		}
	}
</style>
