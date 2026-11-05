<script lang="ts">
	import { connectHint, noAnswer } from '../connect';
	import { connectError, serverHost } from '../fleet';
	import { relative } from '../format';
	import { reach } from '../reach.svelte';
	import { app } from '../state.svelte';
	import { t } from '../i18n';
	import Icon from './Icon.svelte';

	let { inline = false }: { inline?: boolean } = $props();

	/** How long Kubri waits for a cluster before it gives up, as the core does. */
	const WAIT_SECONDS = 15;

	let now = $state(Date.now());
	$effect(() => {
		if (!app.connecting) return;
		now = Date.now();
		const timer = setInterval(() => (now = Date.now()), 1000);
		return () => clearInterval(timer);
	});

	const name = $derived(app.connecting ?? app.connectFailure?.context ?? '');
	/** A context, not a snapshot file being opened. */
	const isContext = $derived(app.contexts.some((c) => c.name === name));
	const known = $derived(reach.of[name] ?? null);
	const host = $derived(serverHost(app.contexts.find((c) => c.name === name)?.server ?? known?.server ?? null));
	const seconds = $derived(Math.max(0, Math.floor((now - app.connectingSince) / 1000)));

	const failure = $derived(app.connecting ? null : app.connectFailure);
	const silent = $derived(failure ? noAnswer(failure.error) : false);
	const detail = $derived.by(() => {
		if (!failure) return '';
		if (silent) {
			return host
				? t("No answer from {host} within {seconds} seconds. Check that you are on the cluster's network or VPN, and that the cluster is running.", { host, seconds: WAIT_SECONDS })
				: t("No answer within {seconds} seconds. Check that you are on the cluster's network or VPN, and that the cluster is running.", { seconds: WAIT_SECONDS });
		}
		return connectHint(failure.error) ?? t('The cluster turned the connection down.');
	});

	function retry() {
		if (failure) void app.switchContext(failure.context);
	}

	function checkClusters() {
		app.connectFailure = null;
		app.showFleet();
	}

	function chooseAnother() {
		app.contextMenuOpen = true;
		app.reloadContexts(true);
	}

	function onkeydown(e: KeyboardEvent) {
		if (e.key === 'Escape' && app.connecting && isContext && !app.dialog) {
			e.preventDefault();
			app.cancelConnect();
		}
	}
</script>

<svelte:window {onkeydown} />

{#if app.connecting}
	<div class="connect-card" class:inline role="status" aria-live="polite">
		<div class="head">
			<span class="spinner" aria-hidden="true"></span>
			<h2>{isContext ? t('Connecting to {name}', { name: app.contextName(name) }) : t('Opening {name}', { name })}</h2>
			{#if isContext}<span class="time" title={t('Kubri gives up after {seconds} seconds', { seconds: WAIT_SECONDS })}>{seconds} s</span>{/if}
		</div>
		{#if isContext}
			{#if host}<p class="muted">{host}</p>{/if}
			{#if known && !known.reachable}
				<p class="warn">
					<Icon name="warning" size={15} />
					{t('{summary} when last tried, {when}', { summary: known.summary, when: relative(new Date(known.at).toISOString(), now) })}{known.error ? `: ${known.error}` : ''}
				</p>
			{/if}
			<div class="buttons">
				<button onclick={() => app.cancelConnect()}>{t('Cancel')} <kbd>esc</kbd></button>
			</div>
		{/if}
	</div>
{:else if failure}
	<div class="connect-card failed" class:inline role="alert">
		<div class="head">
			<Icon name="critical" size={18} />
			<h2>{silent ? t('{name} does not answer', { name: app.contextName(failure.context) }) : t('Could not connect to {name}', { name: app.contextName(failure.context) })}</h2>
			{#if !inline}
				<button class="close" onclick={() => (app.connectFailure = null)} aria-label={t('Dismiss')}>×</button>
			{/if}
		</div>
		<p>{detail}</p>
		<p class="raw">{connectError(failure.error)}</p>
		{#if app.cluster && !inline}
			<p class="muted">{t('Kubri stayed on {name}.', { name: app.contextName(app.cluster.context) })}</p>
		{/if}
		<div class="buttons">
			<button class="primary" onclick={retry}>{t('Try again')}</button>
			{#if app.contexts.length > 1}<button onclick={checkClusters}>{t('Check on the Clusters page')}</button>{/if}
			{#if !app.cluster}<button onclick={chooseAnother}>{t('Choose another context')}</button>{/if}
		</div>
	</div>
{/if}

<style>
	.connect-card {
		position: absolute;
		top: 16px;
		left: 50%;
		z-index: 6;
		width: min(480px, calc(100% - 32px));
		padding: 16px 18px;
		border: 1px solid var(--border);
		border-radius: 12px;
		background: var(--surface-1);
		box-shadow: var(--shadow);
		transform: translateX(-50%);
	}
	.connect-card.inline {
		position: static;
		width: 100%;
		max-width: 520px;
		box-shadow: none;
		transform: none;
		text-align: left;
	}
	.connect-card.failed {
		border-color: color-mix(in srgb, var(--bad) 40%, var(--border));
	}
	.head {
		display: flex;
		align-items: center;
		gap: 10px;
	}
	.failed .head :global(svg) {
		flex: none;
		color: var(--bad);
	}
	h2 {
		flex: 1;
		min-width: 0;
		margin: 0;
		font-size: 15px;
		overflow-wrap: anywhere;
	}
	.time {
		color: var(--text-muted);
		font-variant-numeric: tabular-nums;
		font-size: 13px;
	}
	p {
		margin: 8px 0 0;
		font-size: 13px;
		line-height: 1.5;
	}
	.muted {
		color: var(--text-muted);
	}
	.warn {
		display: flex;
		align-items: flex-start;
		gap: 6px;
		color: var(--warn);
	}
	.warn :global(svg) {
		flex: none;
		margin-top: 2px;
	}
	.raw {
		color: var(--text-muted);
		font-family: var(--mono);
		font-size: 11.5px;
		overflow-wrap: anywhere;
	}
	.buttons {
		display: flex;
		flex-wrap: wrap;
		gap: 8px;
		margin-top: 14px;
	}
	.buttons button {
		display: inline-flex;
		align-items: center;
		gap: 8px;
		padding: 7px 12px;
		border: 1px solid var(--border);
		border-radius: 7px;
		background: var(--surface-1);
		font-size: 13px;
		font-weight: 600;
	}
	.buttons button.primary {
		border-color: var(--accent);
		background: var(--accent);
		color: var(--on-accent);
	}
	kbd {
		padding: 0 4px;
		border: 1px solid var(--border);
		border-radius: 4px;
		color: var(--text-muted);
		font-family: var(--mono);
		font-size: 11px;
		font-weight: 400;
	}
	.close {
		padding: 0 4px;
		border: none;
		background: none;
		color: var(--text-muted);
		font-size: 18px;
		line-height: 1;
	}
	.spinner {
		width: 16px;
		height: 16px;
		flex: none;
		border: 2px solid color-mix(in srgb, var(--accent) 25%, transparent);
		border-top-color: var(--accent);
		border-radius: 50%;
		animation: spin 0.8s linear infinite;
	}
	@keyframes spin {
		to {
			transform: rotate(360deg);
		}
	}
	@media (prefers-reduced-motion: reduce) {
		.spinner {
			animation-duration: 3s;
		}
	}
</style>
