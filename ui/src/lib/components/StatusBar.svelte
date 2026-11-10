<script lang="ts">
	import { backend } from '../api';
	import { footerHints } from '../hints';
	import { t } from '../i18n';
	import { keyFromLabel } from '../menu';
	import { app } from '../state.svelte';

	const hints = $derived(footerHints(app));

	/** Plays a hint's key, so that the hints work as buttons too. */
	function press(key: string) {
		const init = keyFromLabel(key);
		if (init) window.dispatchEvent(new KeyboardEvent('keydown', { ...init, bubbles: true, cancelable: true }));
	}

	/** Keeps the focus where it is, so that the keys work on the list after a click. */
	const keepFocus = (e: MouseEvent) => e.preventDefault();
</script>

<footer>
	<span class="status" class:live={app.cluster && !app.connecting && !app.snapshot} role="status">
		<span class="dot"></span>
		{app.connectingText
			? app.connectingText
			: app.snapshot
				? t('Snapshot · {name}', { name: app.snapshot.file ?? app.snapshot.context })
				: app.cluster
					? t('Live · {name}', { name: app.prefs.contextName(app.cluster.context) })
					: t('Not connected')}
	</span>
	{#if app.releases.latest}
		{@const release = app.releases.latest}
		<span class="release">
			{#if release.installable}
				<button onclick={() => (app.updateOpen = true)} title={t('Update Kubri to {version}', { version: release.version })}>
					{t('Kubri {version} is out', { version: release.version })} · {t('Update')}
				</button>
			{:else}
				<button
					onclick={() => backend.openUrl(release.url)}
					title={release.manual ? `${release.manual} ${t('Opens the release notes and downloads.')}` : t('Open the release notes and downloads')}
				>
					{t('Kubri {version} is out', { version: release.version })}
				</button>
			{/if}
			<button class="close" onclick={() => app.releases.dismiss()} aria-label={t('Hide until the next release')}>×</button>
		</span>
	{/if}
	<span class="hints" data-tour="keys">
		{#each hints as [key, label], i (key + i)}
			{#if keyFromLabel(key)}
				<button onclick={() => press(key)} onmousedown={keepFocus} title="{label} ({key})"><kbd>{key}</kbd>{label}</button>
			{:else}
				<span><kbd>{key}</kbd>{label}</span>
			{/if}
		{/each}
	</span>
</footer>

<style>
	footer {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 4px 16px;
		border-top: 1px solid var(--border);
		background: var(--surface-1);
		font-size: 12px;
		color: var(--text-muted);
	}
	.status {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		flex: none;
		white-space: nowrap;
	}
	.release {
		display: inline-flex;
		align-items: center;
		flex: none;
		border: 1px solid color-mix(in srgb, var(--accent) 40%, var(--border));
		border-radius: 999px;
		background: color-mix(in srgb, var(--accent) 8%, var(--surface-1));
	}
	.release button {
		padding: 1px 4px 1px 10px;
		border: none;
		background: none;
		color: var(--accent);
		font: inherit;
		font-weight: 600;
		cursor: pointer;
	}
	.release .close {
		padding: 1px 9px 1px 4px;
		color: var(--text-muted);
		font-weight: 400;
	}
	.status .dot {
		width: 7px;
		height: 7px;
		border-radius: 50%;
		background: var(--text-muted);
	}
	.status.live {
		color: var(--ok);
	}
	.status.live .dot {
		background: var(--ok);
	}
	.hints {
		display: flex;
		flex-wrap: wrap;
		justify-content: flex-end;
		gap: 2px 4px;
		margin-left: auto;
		min-width: 0;
	}
	.hints > * {
		display: inline-flex;
		align-items: center;
		padding: 2px 6px;
		border: 1px solid transparent;
		border-radius: 6px;
		background: none;
		color: inherit;
		font-size: 12px;
	}
	.hints > button:hover {
		border-color: var(--border);
		background: var(--surface-2);
		color: var(--text);
	}
	kbd,
	kbd {
		margin-right: 5px;
		padding: 0 5px;
		border: 1px solid var(--border);
		border-radius: 4px;
		background: var(--surface-2);
		font-family: var(--mono);
		font-size: 11px;
		color: var(--text);
	}

	@media (max-width: 900px) {
		/* Keys beyond the first few stay in the help. */
		.hints > :nth-child(n + 6):not(:last-child) {
			display: none;
		}
	}
	@media (max-width: 720px) {
		footer {
			order: 4;
		}
		.hints {
			display: none;
		}
	}
</style>
