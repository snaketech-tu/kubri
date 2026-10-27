import { backend, type DeleteOptions, type KubeObject, type ObjectRef, type ResourceInfo } from './api';
import { errorMessage } from './format';
import { app } from './state.svelte';

/** Kinds `kubectl rollout restart` supports. */
export function canRestart(resource: ResourceInfo) {
	return ['Deployment', 'StatefulSet', 'DaemonSet'].includes(resource.kind);
}

export function canScale(resource: ResourceInfo) {
	return resource.subresources.includes('scale');
}

export function canEdit(resource: ResourceInfo) {
	return resource.verbs.includes('update');
}

export function canDelete(resource: ResourceInfo) {
	return resource.verbs.includes('delete');
}

function label(target: ObjectRef) {
	return target.namespace ? `${target.namespace}/${target.name}` : target.name;
}

async function run(success: string, action: () => Promise<unknown>): Promise<boolean> {
	try {
		await action();
		app.toast(success);
		return true;
	} catch (err) {
		app.toast(errorMessage(err), 'bad');
		return false;
	}
}

export async function deleteObjects(resource: ResourceInfo, targets: ObjectRef[], options: DeleteOptions) {
	const results = await Promise.allSettled(targets.map((t) => backend.deleteObject(t, options)));
	const failed = results.filter((r): r is PromiseRejectedResult => r.status === 'rejected');
	if (failed.length) {
		app.toast(`${failed.length} of ${targets.length} deletes failed: ${errorMessage(failed[0].reason)}`, 'bad');
	} else {
		const what = targets.length === 1 ? label(targets[0]) : `${targets.length} ${resource.kind} objects`;
		app.toast(`Deleted ${what}`);
	}
	return failed.length === 0;
}

export function scale(target: ObjectRef, replicas: number) {
	return run(`Scaled ${label(target)} to ${replicas}`, () => backend.scale(target, replicas));
}

export function restart(target: ObjectRef) {
	return run(`Restarting ${label(target)}`, () => backend.restart(target));
}

export function save(target: ObjectRef, value: KubeObject) {
	return run(`Saved ${label(target)}`, () => backend.replaceObject(target, value));
}

/** Merge-patches labels and annotations; keys set to null are removed. */
export function patchMetadata(
	target: ObjectRef,
	labels: Record<string, string | null>,
	annotations: Record<string, string | null>
) {
	return run(`Updated ${label(target)}`, () =>
		backend.patchObject(target, { metadata: { labels, annotations } })
	);
}
