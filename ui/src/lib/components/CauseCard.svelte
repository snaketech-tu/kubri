<script lang="ts">
	import { t } from '../i18n';
	import type { KubeObject, ObjectRef } from '../api';
	import { changeRequestsOf, createMissing, raiseMemoryOf, setImageOf } from '../actions';
	import type { Cause } from '../causes';
	import { app, findResource } from '../state.svelte';
	import Icon from './Icon.svelte';

	let {
		cause,
		target,
		object,
		container = null,
		workload,
		onprevious,
		events = true,
		raw = null
	}: {
		cause: Cause;
		/** The pod or autoscaler the cause is about. */
		target: ObjectRef;
		object: KubeObject;
		container?: string | null;
		/** The workload to change, when it is not the pod's owner, as for an autoscaler. */
		workload?: ObjectRef;
		/** Shows the logs of the instance that ended. */
		onprevious?: () => void;
		/** Offers the object's events besides the fix. */
		events?: boolean;
		/** The reason as Kubernetes names it, shown small beside the sentence. */
		raw?: string | null;
	} = $props();

	let busy = $state(false);
	const changes = $derived(cause.fix === 'image' || cause.fix === 'memory' || cause.fix === 'requests' || cause.fix === 'createConfig');
	/** The fix to offer: a change only where changes are allowed, previous logs only where they can be shown. */
	const fix = $derived.by(() => {
		if (changes && app.readOnly) return null;
		if (cause.fix === 'previousLogs' && !onprevious) return null;
		return cause.fix;
	});

	const label = $derived.by(() => {
		switch (fix) {
			case 'image':
				return t('Set image…');
			case 'memory':
				return t('Raise memory limit…');
			case 'requests':
				return cause.resource === 'memory' ? t('Change memory request…') : t('Change CPU request…');
			case 'previousLogs':
				return t('Read previous logs');
			case 'createConfig':
				return cause.missing ? t('Create {kind} {name}…', { kind: cause.missing.kind, name: cause.missing.name }) : t('Create it…');
			case 'events':
				return t('Show events');
			default:
				return null;
		}
	});

	function showEvents() {
		const resource = findResource(app.resources, target.resource);
		if (resource) app.showDetails(resource, target, 'events');
	}

	async function run() {
		busy = true;
		try {
			if (fix === 'image') await setImageOf(target, object);
			else if (fix === 'memory' && container) await raiseMemoryOf(target, container);
			else if (fix === 'requests') await changeRequestsOf(cause, target, workload ? null : object, workload);
			else if (fix === 'createConfig' && cause.missing) createMissing(cause.missing, target.namespace);
			else if (fix === 'previousLogs') onprevious?.();
			else if (fix === 'events') showEvents();
		} finally {
			busy = false;
		}
	}
</script>

<section class="cause {cause.tone}" role="status">
	<h3>
		<Icon name={cause.tone === 'bad' ? 'critical' : 'pending'} />
		<span class="title">{cause.title}</span>
		{#if raw}<span class="raw">{raw}</span>{/if}
	</h3>
	<p>{cause.detail}</p>
	{#if label || (events && fix !== 'events')}
		<div class="buttons">
			{#if label}<button class="primary" onclick={run} disabled={busy}>{label}</button>{/if}
			{#if events && fix !== 'events'}<button onclick={showEvents}>{t('Show events')}</button>{/if}
		</div>
	{/if}
</section>

<style>
	.cause {
		padding: 14px 16px;
		font-family: var(--sans);
		white-space: normal;
		border: 1px solid;
		border-radius: 9px;
		font-size: 12.5px;
	}
	.cause.bad {
		border-color: color-mix(in srgb, var(--bad) 35%, var(--border));
		background: color-mix(in srgb, var(--bad) 7%, var(--surface-1));
	}
	.cause.warn {
		border-color: color-mix(in srgb, var(--warn) 35%, var(--border));
		background: color-mix(in srgb, var(--warn) 8%, var(--surface-1));
	}
	h3 {
		display: flex;
		align-items: baseline;
		flex-wrap: wrap;
		gap: 4px 8px;
		margin: 0;
		font-size: 13.5px;
		line-height: 1.4;
	}
	h3 :global(svg) {
		align-self: center;
		flex: none;
	}
	.bad h3 :global(svg) {
		color: var(--bad);
	}
	.warn h3 :global(svg) {
		color: var(--warn);
	}
	.title {
		flex: 1 1 200px;
		min-width: 0;
		overflow-wrap: anywhere;
	}
	.raw {
		padding: 1px 6px;
		border-radius: 4px;
		background: var(--surface-2);
		color: var(--text-muted);
		font-family: var(--mono);
		font-size: 11px;
		font-weight: 400;
	}
	p {
		margin: 8px 0 0;
		line-height: 1.5;
		color: var(--text-muted);
		overflow-wrap: anywhere;
	}
	.buttons {
		display: flex;
		flex-wrap: wrap;
		gap: 8px;
		margin-top: 12px;
	}
	button {
		display: inline-flex;
		align-items: center;
		gap: 8px;
		padding: 7px 12px;
		border: 1px solid var(--border);
		border-radius: 7px;
		background: var(--surface-1);
		font-size: 12.5px;
		font-weight: 600;
	}
	button.primary {
		border-color: var(--accent);
		background: var(--accent);
		color: var(--on-accent);
	}
	button:disabled {
		opacity: 0.6;
	}
</style>
