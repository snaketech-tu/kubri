<script lang="ts">
	import { untrack } from 'svelte';
	import type { BenchInfo, BenchRequest, PortForwardInfo } from '../api';
	import { parseHeaders } from '../bench';
	import { t } from '../i18n';
	import { plainOptions } from '../select';
	import Phrase from './Phrase.svelte';
	import Select from './Select.svelte';

	let {
		forward,
		preset,
		firstField = $bindable()
	}: { forward: PortForwardInfo; preset?: BenchInfo; firstField?: HTMLInputElement | HTMLTextAreaElement } = $props();

	// The run to repeat, or the defaults k9s uses (one worker, 200 requests). A dialog is opened
	// afresh for each run, so its first value is the one to fill in.
	const start = untrack(() => preset);
	let method = $state(start?.method ?? 'GET');
	let path = $state(start?.path ?? '/');
	let concurrency = $state<number | null>(start?.concurrency ?? 1);
	let byDuration = $state(!!start?.durationSecs);
	let requests = $state<number | null>(start?.requests ?? 200);
	let seconds = $state<number | null>(start?.durationSecs ?? 30);
	let timeout = $state<number | null>(start?.timeoutSecs ?? 20);
	let headersText = $state(
		Object.entries(start?.headers ?? {})
			.map(([k, v]) => `${k}: ${v}`)
			.join('\n')
	);
	let body = $state(start?.body ?? '');
	const hasBody = $derived(['POST', 'PUT', 'PATCH'].includes(method));

	/** The run as the fields say; throws when a number is out of range. */
	export function request(): BenchRequest {
		const whole = (n: number | null, what: string, max: number) => {
			if (n === null || !Number.isInteger(n) || n < 1 || n > max) throw new Error(t('{what} must be a whole number from 1 to {max}', { what, max }));
			return n;
		};
		return {
			forwardId: forward.id,
			method,
			path: path.trim() || '/',
			headers: parseHeaders(headersText),
			body: hasBody && body ? body : null,
			concurrency: whole(concurrency, t('Concurrency'), 200),
			requests: byDuration ? null : whole(requests, t('Requests'), 1_000_000),
			durationSecs: byDuration ? whole(seconds, t('Duration'), 600) : null,
			timeoutSecs: whole(timeout, t('Timeout'), 120)
		};
	}
</script>

<p>
	<Phrase
		text={t('Send requests through the port-forward to {pod} on port {port}, and measure how fast it answers.', { port: forward.podPort })}
		strong={{ pod: forward.pod }}
	/>
</p>
<div class="request">
	<Select bind:value={method} options={plainOptions(['GET', 'HEAD', 'POST', 'PUT', 'PATCH', 'DELETE', 'OPTIONS'])} aria-label={t('Method')} mono compact />
	<span class="origin">localhost:{forward.localPort}</span>
	<input class="path" bind:value={path} bind:this={firstField} placeholder="/" aria-label={t('Path')} spellcheck="false" />
</div>
<div class="grid">
	<label class="field">
		{t('Concurrency')}
		<input type="number" min="1" max="200" bind:value={concurrency} />
	</label>
	<div class="field">
		<span class="mode" role="group" aria-label={t('Stop after')}>
			<button class:active={!byDuration} onclick={() => (byDuration = false)}>{t('Requests')}</button>
			<button class:active={byDuration} onclick={() => (byDuration = true)}>{t('Seconds')}</button>
		</span>
		{#if byDuration}
			<input type="number" min="1" max="600" bind:value={seconds} aria-label={t('Duration in seconds')} />
		{:else}
			<input type="number" min="1" max="1000000" bind:value={requests} aria-label={t('Number of requests')} />
		{/if}
	</div>
	<label class="field">
		{t('Timeout (s)')}
		<input type="number" min="1" max="120" bind:value={timeout} />
	</label>
</div>
<label class="field">
	{t('Headers')}
	<textarea rows="2" bind:value={headersText} placeholder="Host: shop.example.com" spellcheck="false"></textarea>
</label>
{#if hasBody}
	<label class="field">
		{t('Body')}
		<textarea rows="3" bind:value={body} spellcheck="false"></textarea>
	</label>
{/if}
<p class="muted">{t('Ctrl+Enter starts it. The pod gets real traffic, so mind production.')}</p>

<style>
	p {
		margin: 0 0 10px;
	}
	.muted {
		color: var(--text-muted);
	}
	.field {
		display: flex;
		flex-direction: column;
		gap: 4px;
		margin: 10px 0;
		font-size: 13px;
		color: var(--text-muted);
	}
	textarea {
		padding: 8px 10px;
		border: 1px solid var(--border);
		border-radius: 6px;
		background: var(--bg);
		color: var(--text);
		font-family: var(--mono);
		font-size: 12px;
		resize: vertical;
	}
	.grid {
		display: grid;
		grid-template-columns: 1fr 1fr 1.3fr;
		gap: 10px;
	}
	.grid input {
		width: 100%;
	}
	.request {
		display: flex;
		align-items: center;
		margin: 4px 0 6px;
		border: 1px solid var(--border);
		border-radius: 8px;
		background: var(--bg);
		overflow: hidden;
	}
	/* The method sits in the request bar like a prefix. */
	.request :global(.select) {
		align-self: stretch;
		border: none;
		border-right: 1px solid var(--border);
		border-radius: 0;
		background: var(--surface-2);
		box-shadow: none;
	}
	.request :global(.select:focus-visible),
	.request :global(.select.open) {
		background: color-mix(in srgb, var(--accent) 12%, var(--surface-2));
	}
	.origin {
		padding-left: 8px;
		color: var(--text-muted);
		font-family: var(--mono);
		font-size: 12.5px;
	}
	.path {
		flex: 1;
		min-width: 0;
		padding: 6px 8px 6px 0;
		border: none;
		background: none;
		font-family: var(--mono);
		font-size: 12.5px;
		outline: none;
	}
	.mode {
		display: flex;
		gap: 2px;
	}
	.mode button {
		padding: 0 6px;
		border: none;
		border-radius: 4px;
		background: none;
		color: var(--text-muted);
		font-size: 13px;
	}
	.mode button.active {
		background: var(--surface-2);
		color: var(--text);
		font-weight: 600;
	}
</style>
