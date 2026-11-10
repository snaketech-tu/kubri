<script lang="ts">
	import { backend, type ObjectRef, type ResourceInfo } from '../api';
	import { targetName } from '../action-dialog';
	import { t } from '../i18n';
	import { app } from '../state.svelte';
	import { suggestedLocalPort, type ForwardablePort } from '../pods';
	import Phrase from './Phrase.svelte';

	let {
		resource,
		target,
		ports,
		remotePort = $bindable(),
		localPort = $bindable(),
		address = $bindable(),
		keep = $bindable(),
		firstField = $bindable(),
		intro = true
	}: {
		resource: ResourceInfo;
		target: ObjectRef;
		/** The ports the object declares; null while loading. */
		ports: ForwardablePort[] | null;
		remotePort: number | null;
		localPort: number | null;
		address: string;
		/** Keep the forward as a favorite. */
		keep: boolean;
		firstField?: HTMLInputElement | HTMLTextAreaElement;
		/** Says what the forward goes to; off where the dialog chose it above. */
		intro?: boolean;
	} = $props();

	/** A running forward that listens on the local port already; starting would fail. */
	const taken = $derived(localPort ? app.forwards.find((f) => f.localPort === localPort) : undefined);

	/** Takes a declared port; a local port typed in already stays. */
	function pick(port: number) {
		if (remotePort === null || localPort === suggestedLocalPort(remotePort)) localPort = suggestedLocalPort(port);
		remotePort = port;
	}
</script>

{#if intro}<p><Phrase text={t('Forward a local port to {name}.')} strong={{ name: targetName(target) }} /></p>{/if}
{#if ports === null}
	<p class="muted">{t('Loading ports…')}</p>
{:else if ports.length}
	<div class="ports">
		{#each ports as p (p.port + p.detail)}
			<button type="button" class="port" class:active={remotePort === p.port} onclick={() => pick(p.port)}>
				<strong>{p.port}</strong>
				{#if p.name}<span>{p.name}</span>{/if}
				<span class="muted">{p.detail}</span>
			</button>
		{/each}
	</div>
{:else}
	<p class="muted">{t('No ports are declared; enter one.')}</p>
{/if}
<div class="grid">
	<!-- The local port first: it is the one people change. -->
	<label class="field">
		{t('Local port')}
		<input type="number" min="0" max="65535" bind:value={localPort} bind:this={firstField} placeholder={t('any free port')} />
	</label>
	<label class="field">
		{resource.kind === 'Service' ? t('Service port') : t('Container port')}
		<input type="number" min="1" max="65535" bind:value={remotePort} />
	</label>
	<label class="field">
		{t('Address')}
		<input bind:value={address} placeholder="127.0.0.1" />
	</label>
</div>
{#if taken}
	<p class="warn">{t('Local port {port} is in use by the forward to {target}. Choose another, or leave it empty for any free port.', { port: taken.localPort, target: `${taken.target.resource.plural}/${taken.target.name}` })}</p>
{/if}
{#if address.trim() && !['127.0.0.1', 'localhost', '::1'].includes(address.trim())}
	<p class="warn">{t('Other machines on your network will be able to reach this port.')}</p>
{/if}
<label class="row" title={t('Start it again with one click under Port-forwards in the sidebar')}>
	<input type="checkbox" bind:checked={keep} />
	{t('Keep as a favorite')}
</label>
{#if !backend.desktop}
	<p class="muted">{t('The port opens on the computer kubri-server runs on.')}</p>
{/if}

<style>
	p {
		margin: 0 0 10px;
	}
	.muted {
		color: var(--text-muted);
	}
	.warn {
		color: var(--warn);
		font-size: 13px;
	}
	.row {
		display: flex;
		align-items: center;
		gap: 8px;
		margin: 8px 0;
	}
	.field {
		display: flex;
		flex-direction: column;
		gap: 4px;
		margin: 10px 0;
		font-size: 13px;
		color: var(--text-muted);
	}
	.grid {
		display: grid;
		grid-template-columns: 1fr 1fr 1.3fr;
		gap: 10px;
	}
	.grid input {
		width: 100%;
	}
	.ports {
		display: flex;
		flex-wrap: wrap;
		gap: 6px;
		margin-bottom: 6px;
	}
	.port {
		display: flex;
		gap: 6px;
		align-items: baseline;
		padding: 4px 10px;
		border: 1px solid var(--border);
		border-radius: 6px;
		background: var(--surface-1);
		font-size: 12.5px;
	}
	.port.active {
		border-color: var(--accent);
		background: color-mix(in srgb, var(--accent) 12%, transparent);
	}
</style>
