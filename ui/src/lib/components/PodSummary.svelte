<script lang="ts">
	import { plural, t } from '../i18n';
	import { coreText } from '../i18n/errors';
	import { backend, type KubeObject, type NodeFit, type ObjectRef } from '../api';
	import { changeRequestsOf, createMissing, raiseMemoryOf, setImageOf } from '../actions';
	import { containerCause, pendingCause } from '../causes';
	import { errorMessage } from '../format';
	import { registryOf } from '../pods';
	import { app, findResource } from '../state.svelte';
	import Icon from './Icon.svelte';

	let {
		object,
		target,
		onlogs
	}: {
		object: KubeObject;
		target: ObjectRef;
		/** Opens the logs of a container; `previous` for the instance that ended. */
		onlogs?: (container: string, previous: boolean) => void;
	} = $props();

	interface Problem {
		container: string;
		reason: string;
		message: string | null;
		restarts: number;
		/** The exit code of the instance that ended last. */
		exitCode: number | null;
		/** An instance ended before this one, whose logs Kubernetes keeps. */
		hasPrevious: boolean;
		/** The container ran out of memory, now or last time. */
		oom: boolean;
		/** The image cannot be pulled. */
		pull: boolean;
		init: boolean;
	}

	const statuses = $derived<{ init: boolean; s: any }[]>([
		...(object.status?.initContainerStatuses ?? []).map((s: any) => ({ init: true, s })),
		...(object.status?.containerStatuses ?? []).map((s: any) => ({ init: false, s }))
	]);

	/** Containers that wait for a reason, such as CrashLoopBackOff, or that ended in failure. */
	const problems = $derived.by<Problem[]>(() =>
		statuses.flatMap(({ init, s }): Problem[] => {
			const waiting = s.state?.waiting;
			const terminated = s.state?.terminated;
			const failedNow = terminated && terminated.exitCode !== 0;
			// ContainerCreating and PodInitializing are how every container starts.
			const stuck = waiting?.reason && !/^(ContainerCreating|PodInitializing)$/.test(waiting.reason);
			if (!stuck && !failedNow) return [];
			const reason = waiting?.reason ?? terminated?.reason ?? 'Exited';
			return [
				{
					container: s.name,
					reason,
					message: waiting?.message ?? terminated?.message ?? null,
					restarts: s.restartCount ?? 0,
					exitCode: failedNow ? terminated.exitCode : (s.lastState?.terminated?.exitCode ?? null),
					hasPrevious: !!s.lastState?.terminated,
					oom: terminated?.reason === 'OOMKilled' || s.lastState?.terminated?.reason === 'OOMKilled',
					pull: /^(ErrImagePull|ImagePullBackOff|InvalidImageName|ErrImageNeverPull)$/.test(reason),
					init
				}
			];
		})
	);

	/** Why a pending pod does not start, from its conditions, as the scheduler reports it. */
	const pending = $derived<any[]>(
		object.status?.phase === 'Pending'
			? (object.status?.conditions ?? []).filter((c: any) => c.status === 'False' && c.message)
			: []
	);
	const unscheduled = $derived(pending.some((c) => c.type === 'PodScheduled'));
	const writable = $derived(!app.readOnly);

	function imageOf(p: Problem): string {
		const list = (p.init ? object.spec?.initContainers : object.spec?.containers) ?? [];
		return list.find((c: any) => c.name === p.container)?.image ?? '';
	}

	/** The pull secrets the pod names, and whether each exists; the service account's are added by Kubernetes. */
	let secrets = $state<{ name: string; exists: boolean | null }[]>([]);
	$effect(() => {
		const names: string[] = (object.spec?.imagePullSecrets ?? []).map((s: any) => s.name);
		if (!problems.some((p) => p.pull) || !names.length) {
			secrets = [];
			return;
		}
		let stale = false;
		Promise.all(
			names.map((name) =>
				backend
					.getObject({ ...target, resource: { group: '', version: 'v1', plural: 'secrets' }, name })
					.then(
						() => ({ name, exists: true as boolean | null }),
						(err) => ({ name, exists: /not ?found|404/i.test(errorMessage(err)) ? false : null })
					)
			)
		).then((found) => {
			if (!stale) secrets = found;
		});
		return () => (stale = true);
	});

	let raising = $state(false);
	async function raiseMemory(container: string) {
		raising = true;
		try {
			await raiseMemoryOf(target, container);
		} finally {
			raising = false;
		}
	}

	/** Why a pending pod waits, in plain words. */
	const waiting = $derived(pendingCause(object));

	function showEvents() {
		const resource = findResource(app.resources, target.resource);
		if (resource) app.showDetails(resource, target, 'events');
	}

	let fits = $state<NodeFit[] | null>(null);
	let checking = $state(false);
	$effect(() => {
		void target;
		fits = null;
	});
	async function checkNodes() {
		checking = true;
		try {
			fits = await backend.scheduling(target);
		} catch (err) {
			app.toast(errorMessage(err), 'bad');
		} finally {
			checking = false;
		}
	}
</script>

{#if object.kind === 'Pod' && (problems.length || pending.length)}
	<div class="summary" aria-label={t('Pod problems')}>
		{#each problems as p (p.container)}
			{@const cause = containerCause(object, p.container)}
			<section class="problem bad">
				<h3>
					<Icon name="critical" /><span class="title">{cause?.title ?? t('{reason} in {container}', { reason: p.reason, container: p.container })}</span>
					<span class="raw">{p.oom && p.reason !== 'OOMKilled' ? `${p.reason} · OOMKilled` : p.reason}</span>
				</h3>
				<p>
					{[
						plural(p.restarts, '{n} restart', '{n} restarts'),
						p.exitCode !== null ? t('last exit code {code}', { code: p.exitCode }) : '',
						p.message ?? ''
					]
						.filter(Boolean)
						.join(' · ')}
				</p>
				{#if p.pull}
					{@const image = imageOf(p)}
					<dl class="facts">
						<dt>{t('Image')}</dt>
						<dd><code>{image}</code></dd>
						<dt>{t('Registry')}</dt>
						<dd><code>{registryOf(image)}</code></dd>
						<dt>{t('Pull secrets')}</dt>
						<dd>
							{#each secrets as s (s.name)}
								<code>{s.name}</code>{#if s.exists === false}<span class="missing"> {t('does not exist')}</span>{/if}{' '}
							{:else}
								{object.spec?.serviceAccountName
									? t('None named in the pod; the service account {name} may add some.', { name: object.spec.serviceAccountName })
									: t('None named in the pod.')}
							{/each}
						</dd>
					</dl>
					<p class="muted">
						{t('Check that the image and tag exist, that the registry can be reached from the nodes, and for a private registry, that a pull secret for it exists in this namespace.')}
					</p>
				{/if}
				{#if p.oom}
					<p class="muted">{t('The container used more memory than its limit allows and was killed.')}</p>
				{/if}
				<div class="buttons">
					{#if p.pull && writable}
						<button class="primary" onclick={() => setImageOf(target, object)}>{t('Set image…')}</button>
					{/if}
					{#if cause?.fix === 'createConfig' && cause.missing && writable}
						<button class="primary" onclick={() => createMissing(cause.missing!, target.namespace)}>
							{t('Create {kind} {name}…', { kind: cause.missing.kind, name: cause.missing.name })}
						</button>
					{/if}
					{#if p.pull || cause?.fix === 'createConfig' || cause?.fix === 'events'}
						<button onclick={showEvents}>{t('Show events')}</button>
					{/if}
					{#if p.oom && writable}
						<button class="primary" onclick={() => raiseMemory(p.container)} disabled={raising}>{t('Raise memory limit…')}</button>
					{/if}
					{#if onlogs}
						{#if p.hasPrevious}
							<button class:primary={!p.oom} onclick={() => onlogs(p.container, true)}>
								<Icon name="terminal" size={16} /> {t('Read previous container logs')}
							</button>
						{/if}
						{#if !p.pull}<button onclick={() => onlogs(p.container, false)}>{t('Current logs')}</button>{/if}
					{/if}
					{#if p.hasPrevious && !p.init && !p.pull && writable}
						<button
							onclick={() => (app.dialog = { kind: 'debug', on: 'pod', target, way: 'copy', container: p.container })}
							title={t('A copy of the pod in which {container} sleeps, to look around in it', { container: p.container })}
						>
							{t('Debug copy…')}
						</button>
					{/if}
				</div>
			</section>
		{/each}
		{#if pending.length && !problems.length}
			<section class="problem warn">
				<h3><Icon name="pending" /><span class="title">{waiting?.title ?? t('Waiting to start')}</span></h3>
				{#each pending as c (c.type)}
					<p><strong>{c.reason ?? c.type}</strong> · {c.message}</p>
				{/each}
				{#if unscheduled}
					{#if fits}
						<ul class="nodes" aria-label={t('Nodes')}>
							{#each fits as f (f.node)}
								<li>
									<strong>{f.node}</strong>
									{#if f.reasons.length}
										{f.reasons.map(coreText).join('; ')}
									{:else}
										<span class="muted">{t('Kubri finds nothing against this node; see the scheduler\'s message above.')}</span>
									{/if}
								</li>
							{/each}
						</ul>
						<p class="muted small">{t('Kubri checks the node\'s state, taints, node selector, node affinity, CPU, memory and pod count. Pod affinity, topology spread and volumes are left to the scheduler.')}</p>
					{/if}
					{#if (waiting?.fix === 'requests' && writable) || !fits}
						<div class="buttons">
							{#if waiting?.fix === 'requests' && writable}
								<button class="primary" onclick={() => changeRequestsOf(waiting, target, object)}>
									{waiting.resource === 'memory' ? t('Change memory request…') : t('Change CPU request…')}
								</button>
							{/if}
							{#if !fits}
								<button onclick={checkNodes} disabled={checking}>{checking ? t('Checking…') : t('Why not each node?')}</button>
							{/if}
						</div>
					{/if}
				{/if}
			</section>
		{/if}
	</div>
{/if}

<style>
	.summary {
		display: grid;
		gap: 10px;
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
		flex-wrap: wrap;
		gap: 4px 8px;
		margin: 0;
		font-size: 13.5px;
	}
	.bad h3 {
		color: var(--bad);
	}
	.warn h3 {
		color: var(--warn);
	}
	h3 :global(svg) {
		flex: none;
	}
	h3 .title {
		flex: 1 1 200px;
		min-width: 0;
		color: var(--text);
		overflow-wrap: anywhere;
	}
	h3 .raw {
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
	.facts {
		display: grid;
		grid-template-columns: auto 1fr;
		gap: 4px 12px;
		margin: 10px 0 0;
	}
	.facts dt {
		color: var(--text-muted);
	}
	.facts dd {
		margin: 0;
		overflow-wrap: anywhere;
	}
	code {
		font-family: var(--mono);
		font-size: 12px;
	}
	.missing {
		color: var(--bad);
		font-weight: 600;
	}
	.muted {
		color: var(--text-muted);
	}
	.small {
		font-size: 11.5px;
	}
	.nodes {
		display: grid;
		gap: 4px;
		margin: 10px 0 0;
		padding-left: 18px;
	}
	.nodes strong {
		margin-right: 6px;
		font-family: var(--mono);
	}
	button:disabled {
		opacity: 0.6;
	}
	button.primary {
		border-color: var(--accent);
		background: var(--accent);
		color: var(--on-accent);
	}
</style>
