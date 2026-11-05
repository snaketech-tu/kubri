import {
	backend,
	type ContainerImage,
	type DeleteOptions,
	type GitOpsAction,
	type Hotkey,
	type KubeObject,
	type ObjectRef,
	type Plugin,
	type PluginRun,
	type PortForwardRequest,
	type ResourceInfo,
	type ResourceRef
} from './api';
import { suggest } from './commands';
import { errorMessage } from './format';
import { plural, t } from './i18n';
import { expandVariables, focusVariables } from './k9s';
import { AUTO_PORT_FORWARDS_KEY, annotatedForwards, suggestedLocalPort } from './pods';
import type { Cause } from './causes';
import { app, findResource, type Dialog } from './state.svelte';

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

async function runAction(success: string, action: () => Promise<unknown>, undo?: () => Promise<unknown>): Promise<boolean> {
	try {
		await action();
		app.toast(success);
		app.record({ text: success, ok: true, undo });
		return true;
	} catch (err) {
		app.toast(errorMessage(err), 'bad');
		app.record({ text: t('Failed: {what}', { what: success }), ok: false });
		return false;
	}
}

export async function deleteObjects(resource: ResourceInfo, targets: ObjectRef[], options: DeleteOptions) {
	const results = await Promise.allSettled(targets.map((t) => backend.deleteObject(t, options)));
	const failed = results.filter((r): r is PromiseRejectedResult => r.status === 'rejected');
	const what = targets.length === 1 ? label(targets[0]) : t('{n} {kind} objects', { n: targets.length, kind: resource.kind });
	const failures = t('{failed} of {n} deletes failed', { failed: failed.length, n: targets.length });
	if (failed.length) app.toast(t('{failures}: {error}', { failures, error: errorMessage(failed[0].reason) }), 'bad');
	else app.toast(t('Deleted {what}', { what }));
	app.record({ text: failed.length ? failures : t('Deleted {what}', { what }), ok: failed.length === 0 });
	return failed.length === 0;
}

export async function scale(target: ObjectRef, replicas: number) {
	// What it was, for the undo.
	const before = await backend
		.getObject(target)
		.then((o) => (o.spec as { replicas?: number } | undefined)?.replicas)
		.catch(() => undefined);
	return runAction(
		t('Scaled {name} to {n}', { name: label(target), n: replicas }),
		() => backend.scale(target, replicas),
		typeof before === 'number' ? () => backend.scale(target, before) : undefined
	);
}

export function restart(target: ObjectRef) {
	return runAction(t('Restarting {name}', { name: label(target) }), () => backend.restart(target));
}

export function save(target: ObjectRef, value: KubeObject) {
	return runAction(t('Saved {name}', { name: label(target) }), () => backend.replaceObject(target, value));
}

/** Merge-patches labels and annotations; keys set to null are removed. */
export function patchMetadata(
	target: ObjectRef,
	labels: Record<string, string | null>,
	annotations: Record<string, string | null>
) {
	return runAction(t('Updated {name}', { name: label(target) }), () =>
		backend.patchObject(target, { metadata: { labels, annotations } })
	);
}

export function cordon(node: string, cordoned: boolean) {
	return runAction(
		cordoned ? t('Cordoned {name}', { name: node }) : t('Uncordoned {name}', { name: node }),
		() => backend.cordon(node, cordoned),
		() => backend.cordon(node, !cordoned)
	);
}

/** Runs a CronJob now, like k9s `t`. */
export async function triggerCronJob(target: ObjectRef) {
	try {
		const job = await backend.triggerCronJob(target);
		app.toast(t('Started Job {name}', { name: job }));
		return true;
	} catch (err) {
		app.toast(errorMessage(err), 'bad');
		return false;
	}
}

export function suspendCronJob(target: ObjectRef, suspend: boolean) {
	return runAction(
		suspend ? t('Suspended {name}', { name: label(target) }) : t('Resumed {name}', { name: label(target) }),
		() => backend.suspendCronJob(target, suspend),
		() => backend.suspendCronJob(target, !suspend)
	);
}

/** What a Flux or Argo CD action did, with `{name}` for the object. */
const gitopsDone = (action: GitOpsAction, name: string) =>
	({
		reconcile: () => t('Asked Flux to reconcile {name}', { name }),
		suspend: () => t('Suspended {name}', { name }),
		resume: () => t('Resumed {name}', { name }),
		sync: () => t('Started a sync of {name}', { name }),
		refresh: () => t('Asked Argo CD to compare {name} again', { name }),
		autoSyncOff: () => t('Turned auto-sync off for {name}', { name }),
		autoSyncOn: () => t('Turned auto-sync on for {name}', { name })
	})[action]();

/** A Flux or Argo CD action, as their CLIs run it. */
export function runGitOps(target: ObjectRef, action: GitOpsAction) {
	return runAction(gitopsDone(action, label(target)), () => backend.gitops(target, action));
}

export function renewCertificate(target: ObjectRef) {
	return runAction(t('Asked cert-manager to renew {name}', { name: label(target) }), () => backend.renewCertificate(target));
}

export function setImages(target: ObjectRef, images: ContainerImage[]) {
	return runAction(plural(images.length, 'Set the image of {name}', 'Set the images of {name}', { name: label(target) }), () =>
		backend.setImages(target, images)
	);
}

/**
 * Rolls the Deployment of a ReplicaSet back to it, like k9s Ctrl+L, or the StatefulSet or
 * DaemonSet of a ControllerRevision.
 */
export async function rollback(target: ObjectRef) {
	try {
		const owner = await backend.rollback(target);
		app.toast(t('Rolled {owner} back to {name}', { owner, name: target.name }));
		return true;
	} catch (err) {
		app.toast(errorMessage(err), 'bad');
		return false;
	}
}

/**
 * Copies lines to the clipboard and says what: the line itself, or with `many` how many, like
 * k9s `c` and `n`.
 */
export function copyLines(lines: string[], many?: (n: number) => string) {
	if (!lines.length) return;
	navigator.clipboard.writeText(lines.join('\n')).then(
		() => app.toast(lines.length > 1 && many ? many(lines.length) : t('Copied {text}', { text: lines.join(' ') })),
		(err) => app.toast(errorMessage(err), 'bad')
	);
}

/** Saves text as a file: to Downloads in the desktop app, as a download in the browser. */
export async function saveFile(name: string, text: string) {
	try {
		const path = await backend.saveText(name, text);
		if (path) app.toast(t('Saved {name}', { name: path }));
	} catch (err) {
		app.toast(errorMessage(err), 'bad');
	}
}

/** Starts a port-forward, and with `keep` also keeps it as a favorite of its context. */
export async function startPortForward(request: PortForwardRequest, keep = false) {
	try {
		const f = await backend.startPortForward(request);
		app.toast(t('Forwarding {local} to {pod}', { local: `${f.address}:${f.localPort}`, pod: `${f.pod}:${f.podPort}` }));
		await app.refreshForwards();
		if (keep && !app.favoriteOf(f)) await app.toggleFavorite(f);
		return true;
	} catch (err) {
		app.toast(errorMessage(err), 'bad');
		return false;
	}
}

/**
 * Port-forwards like k9s Shift+F: at once to the ports a `k9scli.io/auto-port-forwards`
 * annotation names, otherwise through the dialog.
 */
export async function portForward(resource: ResourceInfo, target: ObjectRef) {
	let object: KubeObject | null = null;
	try {
		object = await backend.getObject(target);
	} catch {
		// The dialog loads it again and says what is wrong.
	}
	const auto = annotatedForwards(object, AUTO_PORT_FORWARDS_KEY);
	if (!auto.length) {
		app.dialog = { kind: 'portforward', resource, target };
		return;
	}
	const address = app.k9s?.portForwardAddress ?? null;
	for (const { remote, local } of auto) {
		await startPortForward({ target, remotePort: remote, localPort: local ?? suggestedLocalPort(remote), address });
	}
}

export async function stopPortForward(id: number) {
	const ok = await runAction(t('Stopped the port-forward'), () => backend.stopPortForward(id));
	app.refreshForwards();
	return ok;
}

function contextInfo() {
	return app.contexts.find((c) => c.name === app.cluster?.context) ?? null;
}

/** Runs a k9s plugin on what the view has under the cursor, asking first if it wants that. */
export function startPlugin(plugin: Plugin) {
	if (plugin.dangerous && app.readOnly) {
		app.toast(t('{name} changes the cluster, and Kubri is read-only', { name: plugin.description || plugin.name }), 'bad');
		return;
	}
	const focus = app.focus?.() ?? null;
	const run: PluginRun = {
		plugin: plugin.name,
		target: focus?.target ?? null,
		resource: focus?.resource ?? null,
		release: focus?.release ?? null,
		filter: focus?.filter || null,
		columns: focus?.columns ?? []
	};
	// Inputs are asked for in the same dialog, which shows the command as they are filled in.
	if (plugin.confirm || plugin.inputs.length) {
		// The variables Kubri knows here; the rest, like $CONTAINER, are filled in when it runs.
		app.dialog = { kind: 'plugin', plugin, run, variables: focusVariables(focus, contextInfo()) };
		return;
	}
	launchPlugin(plugin, run);
}

/** The plugin's command as it will run, for its confirmation. */
export function pluginCommand(plugin: Plugin, variables: Record<string, string>, run: PluginRun) {
	return [plugin.command, ...plugin.args].map((part) => expandVariables(part, variables, run.columns)).join(' ');
}

export async function launchPlugin(plugin: Plugin, run: PluginRun) {
	if (!plugin.background) {
		app.pluginTerminal = { plugin, run };
		return true;
	}
	return await runAction(t('Started {name}', { name: plugin.description || plugin.name }), () => backend.runPlugin(run));
}

/** Runs a command as if typed after `:`; false if nothing matches it. */
export function runCommand(command: string): boolean {
	const [first] = suggest(command, app.resources, app.contexts, app.namespaces, app.paletteConfig);
	if (first) app.run(first.action);
	return !!first;
}

/** Runs a k9s hotkey's command, which may use the plugin variables. */
export function runHotkey(hotkey: Hotkey) {
	const focus = app.focus?.() ?? null;
	const command = expandVariables(hotkey.command, focusVariables(focus, contextInfo()), focus?.columns ?? []);
	if (!runCommand(command)) app.toast(t('Hotkey {name}: nothing matches “{command}”', { name: hotkey.name, command }), 'bad');
}

/** Kinds that own pods through a pod template, by the kind an owner reference names. */
const WORKLOADS: Record<string, ResourceRef> = {
	Deployment: { group: 'apps', version: 'v1', plural: 'deployments' },
	StatefulSet: { group: 'apps', version: 'v1', plural: 'statefulsets' },
	DaemonSet: { group: 'apps', version: 'v1', plural: 'daemonsets' },
	ReplicaSet: { group: 'apps', version: 'v1', plural: 'replicasets' },
	Job: { group: 'batch', version: 'v1', plural: 'jobs' },
	CronJob: { group: 'batch', version: 'v1', plural: 'cronjobs' }
};

function controllerOf(object: KubeObject) {
	const owners = object.metadata.ownerReferences ?? [];
	return owners.find((o) => o.controller) ?? owners[0] ?? null;
}

/**
 * The workload whose template made the pod: the Deployment above its ReplicaSet, the CronJob
 * above its Job, or the pod itself when nothing owns it. Changing it outlasts the pod.
 */
export async function workloadOf(target: ObjectRef, pod: KubeObject): Promise<ObjectRef> {
	const step = async (at: ObjectRef, object: KubeObject): Promise<ObjectRef> => {
		const owner = controllerOf(object);
		const resource = owner && WORKLOADS[owner.kind];
		if (!owner || !resource) return at;
		const next: ObjectRef = { ...target, resource, name: owner.name };
		if (owner.kind !== 'ReplicaSet' && owner.kind !== 'Job') return next;
		try {
			return await step(next, await backend.getObject(next));
		} catch {
			return next;
		}
	};
	return step(target, pod);
}

/** Opens Set image… on the workload that owns the pod. */
export async function setImageOf(target: ObjectRef, pod: KubeObject) {
	const at = await workloadOf(target, pod);
	const resource = findResource(app.resources, at.resource);
	if (resource) app.dialog = { kind: 'image', resource, target: at };
}

/** Opens Raise memory limit… on the workload that owns the pod. */
export async function raiseMemoryOf(target: ObjectRef, container: string) {
	try {
		const fix = await backend.memoryFix(target, container);
		const resource = fix.workload && findResource(app.resources, fix.workload.resource);
		if (!fix.workload || !resource) {
			app.toast(t('Nothing owns this pod that Kubri can change; raise the limit where the pod is defined'), 'bad');
			return;
		}
		app.dialog = { kind: 'memory', resource, target: fix.workload, fix };
	} catch (err) {
		app.toast(errorMessage(err), 'bad');
	}
}

/** The kinds whose pod template Kubri can change requests and limits in. */
const SIZED = ['deployments', 'statefulsets', 'daemonsets'];

/**
 * Opens Requests & limits… on `target`, or for a pod on the workload that owns it, with `fill`
 * prefilled.
 */
export async function resourcesOf(target: ObjectRef, pod?: KubeObject | null, extra: Pick<Extract<Dialog, { kind: 'resources' }>, 'fill' | 'note'> = {}) {
	try {
		let at = target;
		if (!target.resource.group && target.resource.plural === 'pods') at = await workloadOf(target, pod ?? (await backend.getObject(target)));
		const resource = findResource(app.resources, at.resource);
		if (!resource || at.resource.group !== 'apps' || !SIZED.includes(at.resource.plural)) {
			app.toast(t('Nothing owns this pod that Kubri can change; set the requests where the pod is defined'), 'bad');
			return;
		}
		app.dialog = { kind: 'resources', resource, target: at, ...extra };
	} catch (err) {
		app.toast(errorMessage(err), 'bad');
	}
}

/**
 * Opens the requests of a workload's container for the cause's resource: the workload that owns
 * `pod`, or the one `workload` names, such as an autoscaler's target.
 */
export async function changeRequestsOf(cause: Cause, target: ObjectRef, pod: KubeObject | null, workload?: ObjectRef) {
	await resourcesOf(workload ?? target, workload ? null : pod, {
		fill: [{ container: null, name: cause.resource ?? 'cpu', request: null, limit: null }],
		note: `${cause.title}.`
	});
}

/** Starts a Secret or ConfigMap a container needs, with its key, on the Create page. */
export function createMissing(missing: NonNullable<Cause['missing']>, namespace: string | null) {
	const plural = missing.kind === 'Secret' ? 'secrets' : 'configmaps';
	const resource = findResource(app.resources, { group: '', version: 'v1', plural });
	if (!resource) return;
	const keys = missing.keys.length ? missing.keys : [missing.kind === 'Secret' ? 'password' : 'key'];
	app.showCreate(resource, { name: missing.name, namespace: namespace ?? undefined, data: Object.fromEntries(keys.map((k) => [k, 'change-me'])) });
}
