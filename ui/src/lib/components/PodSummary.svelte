<script lang="ts">
	import { backend, type KubeObject, type NodeFit, type ObjectRef } from '../api';
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
			const fix = await backend.memoryFix(target, container);
			const resource = fix.workload && findResource(app.resources, fix.workload.resource);
			if (!fix.workload || !resource) {
				app.toast('Nothing owns this pod that Kubri can change; raise the limit where the pod is defined', 'bad');
				return;
			}
			app.dialog = { kind: 'memory', resource, target: fix.workload, fix };
		} catch (err) {
			app.toast(errorMessage(err), 'bad');
		} finally {
			raising = false;
		}
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
	<div class="summary" aria-label="Pod problems">
		{#each problems as p (p.container)}
			<section class="problem bad">
				<h3><Icon name="critical" />{p.oom && p.reason !== 'OOMKilled' ? `${p.reason} · OOMKilled` : p.reason} <span>in {p.container}</span></h3>
				<p>
					{p.restarts}
					{p.restarts === 1 ? 'restart' : 'restarts'}{p.exitCode !== null
						? ` · last exit code ${p.exitCode}`
						: ''}{p.message ? ` · ${p.message}` : ''}
				</p>
				{#if p.pull}
					{@const image = imageOf(p)}
					<dl class="facts">
						<dt>Image</dt>
						<dd><code>{image}</code></dd>
						<dt>Registry</dt>
						<dd><code>{registryOf(image)}</code></dd>
						<dt>Pull secrets</dt>
						<dd>
							{#each secrets as s (s.name)}
								<code>{s.name}</code>{#if s.exists === false}<span class="missing"> does not exist</span>{/if}{' '}
							{:else}
								None named in the pod{object.spec?.serviceAccountName ? `; the service account ${object.spec.serviceAccountName} may add some` : ''}.
							{/each}
						</dd>
					</dl>
					<p class="muted">
						Check that the image and tag exist, that the registry can be reached from the nodes, and for a private registry,
						that a pull secret for it exists in this namespace.
					</p>
				{/if}
				{#if p.oom}
					<p class="muted">The container used more memory than its limit allows and was killed.</p>
				{/if}
				<div class="buttons">
					{#if p.oom && writable}
						<button class="primary" onclick={() => raiseMemory(p.container)} disabled={raising}>Raise memory limit…</button>
					{/if}
					{#if onlogs}
						{#if p.hasPrevious}
							<button class:primary={!p.oom} onclick={() => onlogs(p.container, true)}>
								<Icon name="terminal" size={16} /> Read previous container logs
							</button>
						{/if}
						{#if !p.pull}<button onclick={() => onlogs(p.container, false)}>Current logs</button>{/if}
					{/if}
					{#if p.hasPrevious && !p.init && !p.pull && writable}
						<button
							onclick={() => (app.dialog = { kind: 'debug', on: 'pod', target, way: 'copy', container: p.container })}
							title="A copy of the pod in which {p.container} sleeps, to look around in it"
						>
							Debug copy…
						</button>
					{/if}
				</div>
			</section>
		{/each}
		{#if pending.length && !problems.length}
			<section class="problem warn">
				<h3><Icon name="pending" />Waiting to start</h3>
				{#each pending as c (c.type)}
					<p><strong>{c.reason ?? c.type}</strong> · {c.message}</p>
				{/each}
				{#if unscheduled}
					{#if fits}
						<ul class="nodes" aria-label="Nodes">
							{#each fits as f (f.node)}
								<li>
									<strong>{f.node}</strong>
									{#if f.reasons.length}
										{f.reasons.join('; ')}
									{:else}
										<span class="muted">Kubri finds nothing against this node; see the scheduler's message above.</span>
									{/if}
								</li>
							{/each}
						</ul>
						<p class="muted small">Kubri checks the node's state, taints, node selector, node affinity, CPU, memory and pod count. Pod affinity, topology spread and volumes are left to the scheduler.</p>
					{:else}
						<div class="buttons">
							<button onclick={checkNodes} disabled={checking}>{checking ? 'Checking…' : 'Why not each node?'}</button>
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
		gap: 8px;
		margin: 0;
		font-size: 13.5px;
	}
	.bad h3 {
		color: var(--bad);
	}
	.warn h3 {
		color: var(--warn);
	}
	h3 span {
		font-weight: 400;
		color: var(--text-muted);
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
