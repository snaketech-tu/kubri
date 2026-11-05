<script lang="ts">
	import { plural, t } from '../i18n';
	import { backend, type KubeObject, type ObjectRef, type ServiceCheck } from '../api';
	import { app, findResource } from '../state.svelte';
	import Icon from './Icon.svelte';
	import Phrase from './Phrase.svelte';

	let { object, target }: { object: KubeObject; target: ObjectRef } = $props();

	let check = $state<ServiceCheck | null>(null);

	// Checked again when the Service changes, such as after its selector is fixed.
	$effect(() => {
		void object.metadata?.resourceVersion;
		const ref = target;
		let stale = false;
		backend.serviceCheck(ref).then(
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
	<div class="summary" aria-label={t('Service problems')}>
		<section class="problem {check.pods.length && !ready.length ? 'warn' : 'bad'}">
			{#if !check.pods.length}
				<h3><Icon name="critical" />{t('No pod matches the selector')}</h3>
				<p>
					<Phrase
						text={t('The Service selects {selector}, and no pod in {namespace} has all of these labels.', { namespace: target.namespace ?? '' })}
						code={{ selector }}
					/>
				</p>
				{#if check.near.length}
					<p>{t('Pods that nearly match:')}</p>
					<ul>
						{#each check.near as n (n.name)}
							<li><button class="link" onclick={() => openPod(n.name)}>{n.name}</button>: {n.differences.join('; ')}</li>
						{/each}
					</ul>
				{/if}
			{:else if !ready.length}
				<h3><Icon name="pending" />{t('No matching pod is ready')}</h3>
				<p>
					<Phrase
						text={plural(
							check.pods.length,
							'{n} pod matches {selector}, but none passes its readiness check, so the Service sends traffic nowhere. Look at the readiness probes and events of the pods.',
							'{n} pods match {selector}, but none passes its readiness check, so the Service sends traffic nowhere. Look at the readiness probes and events of the pods.'
						)}
						code={{ selector }}
					/>
				</p>
				<ul>
					{#each check.pods.slice(0, 10) as p (p.name)}
						<li><button class="link" onclick={() => openPod(p.name)}>{p.name}</button>{t(': not ready')}</li>
					{/each}
				</ul>
			{/if}
			{#if ports.length}
				<h3 class:second={!ready.length}><Icon name="warning" />{t('Target ports the pods do not have')}</h3>
				<ul>
					{#each ports.slice(0, 10) as p (p.name)}
						<li>
							<button class="link" onclick={() => openPod(p.name)}>{p.name}</button>{t(': no container port named {names} (Service port {ports})', {
								names: p.missingPorts.map((m) => m.split(' → ')[1]).join(', '),
								ports: p.missingPorts.map((m) => m.split(' → ')[0]).join(', ')
							})}
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
	p :global(code) {
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
	/* A soft underline tells links from text without hovering. */
	.link {
		text-decoration: underline;
		text-decoration-color: color-mix(in srgb, currentColor 35%, transparent);
		text-underline-offset: 2px;
	}
	.link:hover {
		text-decoration-color: currentColor;
	}
</style>
