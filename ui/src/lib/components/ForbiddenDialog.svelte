<script lang="ts">
	import { lightDismiss } from '../dismiss';
	import { access, describeCheck } from '../access.svelte';
	import { copyLines } from '../actions';
	import { backend, type Identity } from '../api';
	import { grantYaml } from '../rbac';
	import { untrack } from 'svelte';
	import { app, type Dialog } from '../state.svelte';
	import { plural, t } from '../i18n';
	import Phrase from './Phrase.svelte';

	let { dialog }: { dialog: Extract<Dialog, { kind: 'forbidden' }> } = $props();
	// Read once: the dialog closes by clearing it.
	const { action, check, target } = untrack(() => dialog);

	let box = $state<HTMLDialogElement>();
	let identity = $state<Identity | null>(null);
	let checking = $state(false);

	$effect(() => {
		const previous = document.activeElement as HTMLElement | null;
		box?.showModal();
		return () => previous?.focus();
	});

	$effect(() => {
		backend.whoAmI().then(
			(who) => (identity = who),
			() => (identity = null)
		);
	});

	const yaml = $derived(grantYaml(check, identity));
	const what = $derived(check.subresource ? `${check.resource}/${check.subresource}` : check.resource);

	function close() {
		app.dialog = null;
	}

	function whoMay() {
		close();
		app.rbacFilterRequest = `can ${check.verb} ${what}`;
		app.showRbac(check.namespace ?? undefined);
	}

	/** Asks the cluster again, as after an admin applied the role. */
	async function again() {
		checking = true;
		await access.refresh();
		checking = false;
		if (access.allowed(check) !== false) {
			app.toast(t('{action} is allowed now', { action }));
			close();
		} else {
			app.toast(t('Still not allowed'), 'bad');
		}
	}
</script>

<dialog
	{@attach lightDismiss}
	class="dialog"
	aria-label={t('Not allowed')}
	bind:this={box}
	oncancel={(e) => {
		e.preventDefault();
		close();
	}}
>
	<h2>{t('{action} is not allowed', { action })}</h2>
	<p>
		{#if target}<Phrase text={t('Kubernetes does not let you {action} {name}.', { action: action.toLowerCase() })} strong={{ name: target.name }} />{/if}
		<Phrase text={t('RBAC needs {check}, and none of your roles grant it.')} code={{ check: describeCheck(check).replaceAll('`', '') }} />
	</p>
	<p class="muted">
		{#if identity?.groups.length}
			{@const [head, tail] = plural(
				identity.groups.length,
				'The cluster knows you as {user}, in the group {groups}.',
				'The cluster knows you as {user}, in the groups {groups}.'
			).split('{groups}')}
			<Phrase text={head} code={{ user: identity.username }} />{#each identity.groups as group, i (group)}{i ? ', ' : ''}<code
					>{group}</code
				>{/each}{tail}
		{:else if identity}
			<Phrase text={t('The cluster knows you as {user}.')} code={{ user: identity.username }} />
		{:else}
			{t('The cluster does not say who it takes you for.')}
		{/if}
	</p>
	<div class="grant">
		<div class="grant-head">
			<span>{t('A role that would allow it, to send to an admin')}</span>
			<button class="small" onclick={() => copyLines([yaml])}>{t('Copy YAML')}</button>
		</div>
		<pre>{yaml}</pre>
	</div>
	<div class="buttons">
		<button class="secondary" onclick={whoMay}>{t('Who may do this?')}</button>
		<button class="secondary" onclick={again} disabled={checking}>{checking ? t('Checking…') : t('Check again')}</button>
		<button class="primary" onclick={close}>{t('Close')}</button>
	</div>
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
		margin: 0 0 12px;
		font-size: 17px;
	}
	p {
		margin: 0 0 10px;
		overflow-wrap: anywhere;
	}
	.muted {
		color: var(--text-muted);
	}
	.grant {
		margin: 14px 0;
		border: 1px solid var(--border);
		border-radius: 8px;
		overflow: hidden;
	}
	.grant-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 8px;
		padding: 6px 10px;
		background: var(--surface-2);
		font-size: 12px;
		color: var(--text-muted);
	}
	pre {
		max-height: 260px;
		margin: 0;
		padding: 10px 12px;
		overflow: auto;
		font-family: var(--mono);
		font-size: 12px;
		line-height: 1.5;
	}
	.buttons {
		display: flex;
		flex-wrap: wrap;
		justify-content: flex-end;
		gap: 8px;
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
	.secondary {
		border: 1px solid var(--border);
		background: var(--surface-1);
	}
	.small {
		padding: 2px 8px;
		border: 1px solid var(--border);
		border-radius: 6px;
		background: var(--surface-1);
		font-size: 12px;
	}
</style>
