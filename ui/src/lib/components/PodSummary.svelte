<script lang="ts">
	import type { KubeObject } from '../api';
	import Icon from './Icon.svelte';

	let {
		object,
		onlogs
	}: {
		object: KubeObject;
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
	}

	const statuses = $derived<any[]>([
		...(object.status?.initContainerStatuses ?? []),
		...(object.status?.containerStatuses ?? [])
	]);

	/** Containers that wait for a reason, such as CrashLoopBackOff, or that ended in failure. */
	const problems = $derived.by<Problem[]>(() =>
		statuses.flatMap((s): Problem[] => {
			const waiting = s.state?.waiting;
			const terminated = s.state?.terminated;
			const failedNow = terminated && terminated.exitCode !== 0;
			// ContainerCreating and PodInitializing are how every container starts.
			const stuck = waiting?.reason && !/^(ContainerCreating|PodInitializing)$/.test(waiting.reason);
			if (!stuck && !failedNow) return [];
			return [
				{
					container: s.name,
					reason: waiting?.reason ?? terminated?.reason ?? 'Exited',
					message: waiting?.message ?? terminated?.message ?? null,
					restarts: s.restartCount ?? 0,
					exitCode: failedNow ? terminated.exitCode : (s.lastState?.terminated?.exitCode ?? null),
					hasPrevious: !!s.lastState?.terminated
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
</script>

{#if object.kind === 'Pod' && (problems.length || pending.length)}
	<div class="summary" aria-label="Pod problems">
		{#each problems as p (p.container)}
			<section class="problem bad">
				<h3><Icon name="critical" />{p.reason} <span>in {p.container}</span></h3>
				<p>
					{p.restarts}
					{p.restarts === 1 ? 'restart' : 'restarts'}{p.exitCode !== null
						? ` · last exit code ${p.exitCode}`
						: ''}{p.message ? ` · ${p.message}` : ''}
				</p>
				{#if onlogs}
					<div class="buttons">
						{#if p.hasPrevious}
							<button class="primary" onclick={() => onlogs(p.container, true)}>
								<Icon name="terminal" size={16} /> Read previous container logs
							</button>
						{/if}
						<button onclick={() => onlogs(p.container, false)}>Current logs</button>
					</div>
				{/if}
			</section>
		{/each}
		{#if pending.length && !problems.length}
			<section class="problem warn">
				<h3><Icon name="pending" />Waiting to start</h3>
				{#each pending as c (c.type)}
					<p><strong>{c.reason ?? c.type}</strong> · {c.message}</p>
				{/each}
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
	button.primary {
		border-color: var(--accent);
		background: var(--accent);
		color: var(--on-accent);
	}
</style>
