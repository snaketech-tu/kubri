<script lang="ts">
	import { backend, type KubeObject, type RenderedView, type ResourceInfo } from '../api';
	import { certificateStatus } from '../certs';
	import { gitopsStatus } from '../gitops';
	import { app } from '../state.svelte';
	import HealthIcon from './HealthIcon.svelte';

	let { object, resource, now }: { object: KubeObject; resource: ResourceInfo; now: number } = $props();

	const view = $derived(app.viewOf(resource));
	let rendered = $state<RenderedView | null>(null);

	// Rendered again when the object changes, since health and fields follow its status.
	$effect(() => {
		void object.metadata?.resourceVersion;
		const [v, r, o] = [view, resource, object];
		if (!v) {
			rendered = null;
			return;
		}
		let stale = false;
		backend.renderView(r, o).then(
			(found) => !stale && (rendered = found),
			() => !stale && (rendered = null)
		);
		return () => (stale = true);
	});

	/** Flux, Argo CD and cert-manager have their own status card; their view's health would repeat it. */
	const ownCard = $derived(!!(gitopsStatus(resource, object) ?? certificateStatus(resource, object, now)));
	const health = $derived(ownCard ? null : rendered?.health);
	const sections = $derived(rendered?.sections.filter((s) => s.fields.length) ?? []);
</script>

{#if health || sections.length}
	<div class="summary" aria-label="{resource.kind} view">
		<section class="card {health?.health ?? ''}">
			{#if health}
				<h3>
					<HealthIcon health={health.health} size={11} />
					{health.health === 'ok' ? 'Healthy' : health.health === 'failed' ? 'Failed' : 'Needs a look'}
				</h3>
				{#if health.message}<p class="message">{health.message}</p>{/if}
			{/if}
			{#each sections as section, i (i)}
				<h4 class:first={!health && i === 0}>{section.title}</h4>
				<dl>
					{#each section.fields as [name, value], j (j)}
						<dt>{name}</dt>
						<dd>{value}</dd>
					{/each}
				</dl>
			{/each}
			<p class="source" title="Views are YAML files; see Config → Views">From the view in {view?.source}</p>
		</section>
	</div>
{/if}

<style>
	.summary {
		padding: 16px 20px 0;
	}
	.card {
		padding: 14px 16px;
		border: 1px solid var(--border);
		border-radius: 9px;
		background: var(--surface-1);
		font-size: 12.5px;
	}
	.card.failed {
		border-color: color-mix(in srgb, var(--bad) 35%, var(--border));
		background: color-mix(in srgb, var(--bad) 7%, var(--surface-1));
	}
	.card.warning {
		border-color: color-mix(in srgb, var(--warn) 35%, var(--border));
		background: color-mix(in srgb, var(--warn) 8%, var(--surface-1));
	}
	h3 {
		display: flex;
		align-items: center;
		gap: 8px;
		margin: 0;
		font-size: 13.5px;
	}
	h4 {
		margin: 14px 0 4px;
		font-size: 12.5px;
	}
	h4.first {
		margin-top: 0;
	}
	.message {
		margin: 8px 0 0;
		line-height: 1.5;
		overflow-wrap: anywhere;
	}
	dl {
		display: grid;
		grid-template-columns: max-content 1fr;
		gap: 3px 14px;
		margin: 6px 0 0;
	}
	dt {
		color: var(--text-muted);
	}
	dd {
		margin: 0;
		min-width: 0;
		font-family: var(--mono);
		font-size: 12px;
		overflow-wrap: anywhere;
		white-space: pre-wrap;
	}
	.source {
		margin: 12px 0 0;
		color: var(--text-muted);
		font-size: 11.5px;
	}
</style>
