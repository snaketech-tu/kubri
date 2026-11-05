<script lang="ts">
	import { lightDismiss } from '../dismiss';
	import { drafts } from '../navigation.svelte';
	import { t } from '../i18n';
	let box = $state<HTMLDialogElement>();
	$effect(() => {
		if (drafts.pending) box?.showModal();
		else box?.close();
	});
</script>

<dialog
	{@attach lightDismiss}
	bind:this={box}
	oncancel={(e) => {
		e.preventDefault();
		if (!drafts.saving) drafts.pending = null;
	}}
	aria-labelledby="draft-title"
>
	<h2 id="draft-title">{t('Keep your YAML changes?')}</h2>
	<p>{t('Your changes have not been saved. Save them before continuing, or keep editing.')}</p>
	{#if drafts.failed}
		<p class="failed" role="alert">{t('Saving did not work. Keep editing to see why, or discard your changes.')}</p>
	{/if}
	<div class="buttons">
		<button disabled={drafts.saving} onclick={() => (drafts.pending = null)}>{t('Keep editing')}</button>
		<button disabled={drafts.saving} onclick={() => drafts.proceed('discard')}>{t('Discard changes')}</button>
		<button class="primary" disabled={drafts.saving} onclick={() => drafts.proceed('save')}
			>{drafts.saving ? t('Saving…') : t('Save and continue')}</button
		>
	</div>
</dialog>

<style>
	dialog {
		width: min(480px, calc(100vw - 32px));
		border: 1px solid var(--border);
		border-radius: 12px;
		padding: 24px;
		background: var(--surface-1);
		color: var(--text);
		box-shadow: var(--shadow);
	}
	dialog::backdrop {
		background: #10152288;
	}
	h2 {
		margin: 0 0 12px;
		font-size: 20px;
	}
	p {
		color: var(--text-muted);
		line-height: 1.6;
	}
	.failed {
		color: var(--bad);
	}
	.buttons {
		display: flex;
		justify-content: flex-end;
		gap: 8px;
		flex-wrap: wrap;
		margin-top: 24px;
	}
	button {
		padding: 8px 12px;
		border: 1px solid var(--border);
		border-radius: 7px;
		background: var(--surface-2);
	}
	.primary {
		background: var(--accent);
		color: var(--on-accent);
		border-color: var(--accent);
	}
</style>
