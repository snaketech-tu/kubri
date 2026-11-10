<script lang="ts">
	import { isTauri } from '@tauri-apps/api/core';
	import { backend } from '../api';
	import logo from '../assets/logo.svg';
	import { t } from '../i18n';
	import { openDemo, retryKubeconfig } from '../start';
	import { app } from '../state.svelte';
	import ConnectCard from './ConnectCard.svelte';
	import Icon from './Icon.svelte';
	import LocalClusterDialog from './LocalClusterDialog.svelte';

	/** The dialog that makes a cluster on this computer, from the page shown without a kubeconfig. */
	let localClusterOpen = $state(false);
</script>

<!-- Without a cluster: how to get one, connecting to one, or choosing one. -->
{#if app.kubeconfigProblem !== null && !app.contexts.length && !app.connecting}
	<section class="connect">
		<img src={logo} alt="" width="56" height="56" />
		<span class="eyebrow">{t('Getting started')}</span>
		<h1>{t('No clusters yet.')}</h1>
		<p>
			{isTauri()
				? t('Kubri reads your clusters from a kubeconfig, as kubectl does, and found none it can use. Load the kubeconfig file you got for your cluster, or start a cluster on this computer.')
				: t('kubri-server found no kubeconfig it can use. Add contexts from a kubeconfig file, or start the server with one.')}
			{t('To look around first, open the demo: a snapshot of a small shop, read-only.')}
		</p>
		<p class="problem">{app.kubeconfigProblem}</p>
		<div class="connect-actions">
			<button class="primary" onclick={() => (app.kubeconfigFilesOpen = isTauri() ? true : 'copy')}>
				{isTauri() ? t('Load kubeconfig file…') : t('Add contexts from a file…')}
				<Icon name="arrow" size={16} />
			</button>
			{#await backend.localClusters().catch(() => null) then clusters}
				{#if clusters?.allowed}<button onclick={() => (localClusterOpen = true)}>{t('Start a cluster…')}</button>{/if}
			{/await}
			<button onclick={openDemo}>{t('Open the demo')}</button>
			<button onclick={retryKubeconfig}>{t('Try again')}</button>
		</div>
		{#if localClusterOpen}<LocalClusterDialog onclose={() => (localClusterOpen = false)} />{/if}
		<p class="small">
			{t('Kubri looks in the files listed in')} <code>KUBECONFIG</code>{t(', or in')} <code>~/.kube/config</code>.
		</p>
	</section>
{:else if app.connecting || app.connectFailure}
	<section class="connect">
		<img src={logo} alt="" width="56" height="56" />
		<ConnectCard inline />
		{#if !app.connecting}
			<div class="connect-actions">
				<button onclick={() => (app.kubeconfigFilesOpen = true)}>{t('Kubeconfig files…')}</button>
			</div>
		{/if}
	</section>
{:else if app.loaded}
	<section class="connect">
		<img src={logo} alt="" width="56" height="56" />
		<h1>{t('Connect to a cluster.')}</h1>
		<p>
			{isTauri()
				? t('Choose a context from your kubeconfig. Reload it after you update your credentials.')
				: t('Choose a context configured on kubri-server. The server holds the credentials.')}
		</p>
		<div class="connect-actions">
			<button
				class="primary"
				onclick={() => {
					app.contextMenuOpen = true;
					app.reloadContexts(true);
				}}
			>
				{t('Choose context')} <Icon name="arrow" size={16} />
			</button>
			<button onclick={() => app.load()}>{t('Try again')}</button>
			<button onclick={() => (app.kubeconfigFilesOpen = true)}>{t('Kubeconfig files…')}</button>
			{#if app.contexts.length > 1}<button onclick={() => app.showFleet()}>{t('See all clusters')}</button>{/if}
		</div>
		<p class="small">{t('Your k9s aliases, hotkeys, plugins and skins keep working.')}</p>
	</section>
{/if}

<style>
	.connect {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 10px;
		padding: 32px 20px;
		text-align: center;
		overflow: auto;
	}
	.eyebrow {
		margin-top: 10px;
		font-size: 11px;
		font-weight: 650;
		letter-spacing: 0.12em;
		text-transform: uppercase;
		color: var(--text-muted);
	}
	.connect h1 {
		margin: 0;
		font-size: 32px;
		letter-spacing: -0.03em;
	}
	.connect p {
		max-width: 460px;
		margin: 0;
		color: var(--text-muted);
		line-height: 1.6;
	}
	.connect .problem {
		max-width: 560px;
		padding: 8px 12px;
		border-radius: 8px;
		background: color-mix(in srgb, var(--bad) 8%, transparent);
		color: var(--bad);
		font-size: 13px;
		overflow-wrap: anywhere;
	}
	.connect .small {
		margin-top: 18px;
		font-size: 12px;
	}
	.connect-actions {
		display: flex;
		flex-wrap: wrap;
		justify-content: center;
		gap: 8px;
		margin-top: 14px;
	}
	.connect-actions button {
		display: inline-flex;
		align-items: center;
		gap: 8px;
		padding: 9px 16px;
		border: 1px solid var(--border);
		border-radius: 8px;
		background: var(--surface-1);
		font-weight: 600;
	}
	.connect-actions .primary {
		border-color: var(--accent);
		background: var(--accent);
		color: var(--on-accent);
	}
	@media (max-width: 720px) {
		.connect h1 {
			font-size: 26px;
		}
	}
</style>
