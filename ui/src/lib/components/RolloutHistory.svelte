<script lang="ts">
	import { backend, type ObjectRef, type ResourceInfo } from '../api';
	import { errorMessage } from '../format';
	import { age } from '../format';
	import { imageChanges, revisionsOf, type Revision } from '../rollout';
	import { app, findResource } from '../state.svelte';

	let { target, now }: { target: ObjectRef; now: number } = $props();

	const MAX = 10;
	let revisions = $state<Revision[] | { error: string } | 'loading'>('loading');
	const replicaSets = $derived<ResourceInfo | undefined>(findResource(app.resources, { group: 'apps', version: 'v1', plural: 'replicasets' }));
	const rollbackAllowed = $derived(!app.guard.readOnly);

	$effect(() => {
		const dep = target;
		const rsResource = replicaSets;
		revisions = 'loading';
		if (!rsResource) return;
		let current = true;
		let stop: () => void = () => {};
		(async () => {
			try {
				const deployment = await backend.getObject(dep);
				const uid = deployment.metadata.uid;
				const names = await new Promise<string[]>((resolve, reject) => {
					stop = backend.watchTable(rsResource, dep.namespace, null, (event) => {
						if (event.type === 'reset') resolve(event.rows.filter((r) => r.owners.some((o) => o.uid === uid)).map((r) => r.name));
						else if (event.type === 'error') reject(new Error(event.message));
					});
				});
				stop();
				const objects = await Promise.all(names.map((name) => backend.getObject({ ...dep, resource: rsResource, name })));
				if (current) revisions = revisionsOf(deployment, objects).slice(0, MAX);
			} catch (err) {
				if (current) revisions = { error: errorMessage(err) };
			}
		})();
		return () => {
			current = false;
			stop();
		};
	});

	function rollBack(revision: Revision) {
		if (!replicaSets) return;
		app.dialog = { kind: 'rollback', resource: replicaSets, target: { ...target, resource: replicaSets, name: revision.name } };
	}
</script>

{#if revisions !== 'loading' && !('error' in revisions) && revisions.length > 1}
	<section class="history">
		<h3>Rollout history</h3>
		<ul>
			{#each revisions as r, i (r.revision)}
				{@const next = revisions[i + 1]}
				{@const changes = next ? imageChanges(next.images, r.images) : []}
				<li>
					<div class="line">
						<strong>Revision {r.revision}</strong>
						{#if r.current}<span class="now">current</span>{/if}
						{#if r.created}<span class="muted">{age(r.created, now)} ago</span>{/if}
						{#if !r.current && rollbackAllowed}
							<button class="btn small" onclick={() => rollBack(r)}>Roll back to revision {r.revision}</button>
						{/if}
					</div>
					<div class="images">
						{#if changes.length}
							{#each changes as c (c)}<code>{c}</code>{/each}
						{:else}
							{#each r.images as image (image)}<code>{image}</code>{/each}
						{/if}
					</div>
					{#if r.cause}<div class="muted">{r.cause}</div>{/if}
				</li>
			{/each}
		</ul>
	</section>
{/if}

<style>
	.history {
		margin: 16px 20px 0;
	}
	h3 {
		font-size: 13px;
		margin: 0 0 8px;
	}
	ul {
		list-style: none;
		margin: 0;
		padding: 0;
		display: grid;
		gap: 10px;
	}
	li {
		border: 1px solid var(--border);
		border-radius: 8px;
		padding: 8px 10px;
	}
	.line {
		display: flex;
		align-items: center;
		gap: 8px;
		flex-wrap: wrap;
	}
	.line .btn {
		margin-left: auto;
	}
	.now {
		color: var(--ok, #1a7f37);
		font-size: 12px;
	}
	.muted {
		color: var(--text-muted);
		font-size: 12px;
	}
	.images {
		display: flex;
		flex-direction: column;
		gap: 2px;
		margin-top: 4px;
	}
	code {
		font-family: var(--mono);
		font-size: 12px;
		overflow-wrap: anywhere;
	}
</style>
