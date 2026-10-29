<script lang="ts">
	import { backend, type KubeObject, type ObjectRef, type ServiceCheck } from '../api';
	import { app, findResource } from '../state.svelte';
	import Icon from './Icon.svelte';

	let { object, target }: { object: KubeObject; target: ObjectRef } = $props();

	let check = $state<ServiceCheck | null>(null);

	// Checked again when the Service changes, such as after its selector is fixed.
	$effect(() => {
		void object.metadata?.resourceVersion;
		const t = target;
		let stale = false;
		backend.serviceCheck(t).then(
			(c) => !stale && (check = c),
			() => !stale && (check = null)
		);
		return () => (stale = true);
	});

	const selector = $derived(
		Object.entries(check?.selector ?? {})
			.map(([k, v]) => `${k}=${v}`)
			.join(', ')
	);
	const ready = $derived(check?.pods.filter((p) => p.ready) ?? []);
	const ports = $derived(check?.pods.filter((p) => p.missingPorts.length) ?? []);
	/** ExternalName Services and those without a selector have endpoints managed elsewhere. */
	const managedElsewhere = $derived(object.spec?.type === 'ExternalName' || !Object.keys(object.spec?.selector ?? {}).length);
	const problem = $derived(!!check && !managedElsewhere && (!ready.length || ports.length > 0));

	function openPod(name: string) {
		const pods = findResource(app.resources, { group: '', version: 'v1', plural: 'pods' });
		if (pods) app.showDetails(pods, { ...target, resource: { group: '', version: 'v1', plural: 'pods' }, name });
	}
</script>

{#if check && problem}
	<div class="summary" aria-label="Service problems">
		<section class="problem {check.pods.length && !ready.length ? 'warn' : 'bad'}">
			{#if !check.pods.length}
				<h3><Icon name="critical" />No pod matches the selector</h3>
				<p>The Service selects <code>{selector}</code>, and no pod in {target.namespace} has all of these labels.</p>
				{#if check.near.length}
					<p>Pods that nearly match:</p>
					<ul>
						{#each check.near as n (n.name)}
							<li><button class="link" onclick={() => openPod(n.name)}>{n.name}</button>: {n.differences.join('; ')}</li>
						{/each}
					</ul>
				{/if}
			{:else if !ready.length}
				<h3><Icon name="pending" />No matching pod is ready</h3>
				<p>
					{check.pods.length}
					{check.pods.length === 1 ? 'pod matches' : 'pods match'} <code>{selector}</code>, but none passes its readiness check, so
					the Service sends traffic nowhere. Look at the pods' readiness probes and events.
				</p>
				<ul>
					{#each check.pods.slice(0, 10) as p (p.name)}
						<li><button class="link" onclick={() => openPod(p.name)}>{p.name}</button>: not ready</li>
					{/each}
				</ul>
			{/if}
			{#if ports.length}
				<h3 class:second={!ready.length}><Icon name="warning" />Target ports the pods do not have</h3>
				<ul>
					{#each ports.slice(0, 10) as p (p.name)}
						<li>
							<button class="link" onclick={() => openPod(p.name)}>{p.name}</button> has no container port named
							{p.missingPorts.map((m) => m.split(' → ')[1]).join(', ')} (Service port {p.missingPorts.map((m) => m.split(' → ')[0]).join(', ')})
						</li>
					{/each}
				</ul>
			{/if}
		</section>
	</div>
{/if}

<style>
	.summary {
		padding: 16px 20px 0;
	}
	.problem {
		padding: 14px 16px;
		border: 1px solid;
		border-radius: 9px;
		font-size: 12.5px;
	}
	.problem.bad {
		border-color: color-mix(in srgb, var(--bad) 35%, var(--border));
		background: color-mix(in srgb, var(--bad) 7%, var(--surface-1));
	}
	.problem.warn {
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
	h3.second {
		margin-top: 14px;
	}
	.bad h3 {
		color: var(--bad);
	}
	.warn h3 {
		color: var(--warn);
	}
	p {
		margin: 8px 0 0;
		line-height: 1.5;
		overflow-wrap: anywhere;
	}
	ul {
		margin: 6px 0 0;
		padding-left: 18px;
		line-height: 1.6;
	}
	code {
		font-family: var(--mono);
		font-size: 12px;
	}
	.link {
		padding: 0;
		border: none;
		background: none;
		color: var(--accent);
		font-family: var(--mono);
		font-size: 12px;
		cursor: pointer;
	}
	.link:hover {
		text-decoration: underline;
	}
</style>
