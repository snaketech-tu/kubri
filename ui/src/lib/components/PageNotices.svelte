<script lang="ts">
	import { age } from '../format';
	import { t } from '../i18n';
	import { closeSnapshot } from '../start';
	import { app } from '../state.svelte';
	import Icon from './Icon.svelte';
</script>

{#if app.snapshot}
	{@const snapshot = app.snapshot}
	<div class="snapshot-banner" role="note">
		<Icon name="camera" size={15} />
		<span>
			<strong>{t('Snapshot')}</strong> {t('of')} {snapshot.context} · {snapshot.namespace ? t('namespace {name}', { name: snapshot.namespace }) : t('the whole cluster')}
			· {t('taken {when} ({age} ago)', { when: new Date(snapshot.taken).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' }), age: age(new Date(snapshot.taken).toISOString(), Date.now()) || '0s' })}
			· {t('read-only, nothing here is live')}
			{#if snapshot.leftOut.length}
				· <span class="left-out" title={snapshot.leftOut.join('\n')}>{t('{n} left out', { n: snapshot.leftOut.length })}</span>
			{/if}
		</span>
		<button onclick={() => (app.contexts.length ? app.leaveSnapshot() : closeSnapshot())}>{t('Leave snapshot')}</button>
	</div>
{/if}
{#if app.prefs.error}
	<div class="error" role="alert">
		<span>{t("Kubri's settings file could not be read, so it uses the defaults: {error}", { error: app.prefs.error })}</span>
		<button onclick={() => (app.prefs.error = null)} aria-label={t('Dismiss')}>×</button>
	</div>
{/if}
{#if app.error}
	<div class="error" role="alert">
		<span>{app.error}</span>
		<button onclick={() => (app.error = null)} aria-label={t('Dismiss')}>×</button>
	</div>
{/if}

<style>
	.snapshot-banner {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 5px 12px 5px 20px;
		background: color-mix(in srgb, var(--accent) 12%, var(--surface-1));
		color: var(--text);
		font-size: 12.5px;
	}
	.snapshot-banner span {
		flex: 1;
		min-width: 0;
	}
	.snapshot-banner .left-out {
		text-decoration: underline dotted;
		cursor: help;
	}
	.snapshot-banner button {
		flex: none;
		padding: 3px 10px;
		border: 1px solid var(--border);
		border-radius: 6px;
		background: var(--surface-1);
		font-size: 12px;
	}
	.error {
		display: flex;
		justify-content: space-between;
		gap: 12px;
		padding: 8px 20px;
		background: color-mix(in srgb, var(--bad) 14%, var(--surface-1));
		color: var(--bad);
	}
	.error button {
		border: none;
		background: none;
		font-size: 18px;
		line-height: 1;
	}
	/* The page, and the workspace below or beside it. */
	@media (max-width: 720px) {
		.error {
			order: 2;
		}
	}
</style>
