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
import { expandVariables, focusVariables } from './k9s';
import { AUTO_PORT_FORWARDS_KEY, annotatedForwards, suggestedLocalPort } from './pods';
import type { Cause } from './causes';
import { app, findResource } from './state.svelte';

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
		app.record({ text: `Failed: ${success}`, ok: false });
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
	app.record({ text: failed.length ? `${failed.length} of ${targets.length} deletes failed` : `Deleted ${targets.length === 1 ? label(targets[0]) : `${targets.length} ${resource.kind} objects`}`, ok: failed.length === 0 });
	return failed.length === 0;
}

export async function scale(target: ObjectRef, replicas: number) {
	// What it was, for the undo.
	const before = await backend
		.getObject(target)
		.then((o) => (o.spec as { replicas?: number } | undefined)?.replicas)
		.catch(() => undefined);
	return runAction(
		`Scaled ${label(target)} to ${replicas}`,
		() => backend.scale(target, replicas),
		typeof before === 'number' ? () => backend.scale(target, before) : undefined
	);
}

export function restart(target: ObjectRef) {
	return runAction(`Restarting ${label(target)}`, () => backend.restart(target));
}

export function save(target: ObjectRef, value: KubeObject) {
	return runAction(`Saved ${label(target)}`, () => backend.replaceObject(target, value));
}

/** Merge-patches labels and annotations; keys set to null are removed. */
export function patchMetadata(
	target: ObjectRef,
	labels: Record<string, string | null>,
	annotations: Record<string, string | null>
) {
	return runAction(`Updated ${label(target)}`, () =>
		backend.patchObject(target, { metadata: { labels, annotations } })
	);
}

export function cordon(node: string, cordoned: boolean) {
	return runAction(
		cordoned ? `Cordoned ${node}` : `Uncordoned ${node}`,
		() => backend.cordon(node, cordoned),
		() => backend.cordon(node, !cordoned)
	);
}

/** Runs a CronJob now, like k9s `t`. */
export async function triggerCronJob(target: ObjectRef) {
	try {
		const job = await backend.triggerCronJob(target);
		app.toast(`Started Job ${job}`);
		return true;
	} catch (err) {
		app.toast(errorMessage(err), 'bad');
		return false;
	}
}

export function suspendCronJob(target: ObjectRef, suspend: boolean) {
	return runAction(
		suspend ? `Suspended ${label(target)}` : `Resumed ${label(target)}`,
		() => backend.suspendCronJob(target, suspend),
		() => backend.suspendCronJob(target, !suspend)
	);
}

const GITOPS_DONE: Record<GitOpsAction, string> = {
	reconcile: 'Asked Flux to reconcile',
	suspend: 'Suspended',
	resume: 'Resumed',
	sync: 'Started a sync of',
	refresh: 'Asked Argo CD to compare again',
	autoSyncOff: 'Turned auto-sync off for',
	autoSyncOn: 'Turned auto-sync on for'
};

/** A Flux or Argo CD action, as their CLIs run it. */
export function runGitOps(target: ObjectRef, action: GitOpsAction) {
	return runAction(`${GITOPS_DONE[action]} ${label(target)}`, () => backend.gitops(target, action));
}

export function renewCertificate(target: ObjectRef) {
	return runAction(`Asked cert-manager to renew ${label(target)}`, () => backend.renewCertificate(target));
}

export function setImages(target: ObjectRef, images: ContainerImage[]) {
	return runAction(`Set ${images.length === 1 ? 'the image' : 'images'} of ${label(target)}`, () =>
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
		app.toast(`Rolled ${owner} back to ${target.name}`);
		return true;
	} catch (err) {
		app.toast(errorMessage(err), 'bad');
		return false;
	}
}

/** Copies lines to the clipboard and says how many, like k9s `c` and `n`. */
export function copyLines(lines: string[], what: [string, string]) {
	if (!lines.length) return;
	navigator.clipboard.writeText(lines.join('\n')).then(
		() => app.toast(lines.length === 1 ? `Copied ${lines[0]}` : `Copied ${lines.length} ${what[1]}`),
		(err) => app.toast(errorMessage(err), 'bad')
	);
}

/** Saves text as a file: to Downloads in the desktop app, as a download in the browser. */
export async function saveFile(name: string, text: string) {
	try {
		const path = await backend.saveText(name, text);
		if (path) app.toast(`Saved ${path}`);
	} catch (err) {
		app.toast(errorMessage(err), 'bad');
	}
}

export async function startPortForward(request: PortForwardRequest) {
	try {
		const f = await backend.startPortForward(request);
		app.toast(`Forwarding ${f.address}:${f.localPort} to ${f.pod}:${f.podPort}`);
		app.refreshForwards();
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
	const ok = await runAction('Stopped the port-forward', () => backend.stopPortForward(id));
	app.refreshForwards();
	return ok;
}

function contextInfo() {
	return app.contexts.find((c) => c.name === app.cluster?.context) ?? null;
}

/** Runs a k9s plugin on what the view has under the cursor, asking first if it wants that. */
export function startPlugin(plugin: Plugin) {
	if (plugin.dangerous && app.readOnly) {
		app.toast(`${plugin.description || plugin.name} changes the cluster, and Kubri is read-only`, 'bad');
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
	return await runAction(`Started ${plugin.description || plugin.name}`, () => backend.runPlugin(run));
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
	if (!runCommand(command)) app.toast(`Hotkey ${hotkey.name}: nothing matches “${command}”`, 'bad');
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
			app.toast('Nothing owns this pod that Kubri can change; raise the limit where the pod is defined', 'bad');
			return;
		}
		app.dialog = { kind: 'memory', resource, target: fix.workload, fix };
	} catch (err) {
		app.toast(errorMessage(err), 'bad');
	}
}

/**
 * Opens the requests of a workload's container for the cause's resource: the workload that owns
 * `pod`, or the one `workload` names, such as an autoscaler's target.
 */
export async function changeRequestsOf(cause: Cause, target: ObjectRef, pod: KubeObject | null, workload?: ObjectRef) {
	const name = cause.resource ?? 'cpu';
	try {
		const at = workload ?? (pod ? await workloadOf(target, pod) : target);
		const resource = findResource(app.resources, at.resource);
		// Requests are changed through the pod template, which only these kinds have.
		if (!resource || !['deployments', 'statefulsets', 'daemonsets'].includes(at.resource.plural)) {
			app.toast('Nothing owns this pod that Kubri can change; set the requests where the pod is defined', 'bad');
			return;
		}
		const object = await backend.getObject(at);
		const containers: { name: string; resources?: { requests?: Record<string, string>; limits?: Record<string, string> } }[] =
			(object.spec as { template?: { spec?: { containers?: [] } } } | undefined)?.template?.spec?.containers ?? [];
		const container = containers.find((c) => c.resources?.requests?.[name]) ?? containers[0];
		if (!container) return;
		const request = container.resources?.requests?.[name] ?? null;
		const limit = container.resources?.limits?.[name] ?? null;
		app.dialog = {
			kind: 'resources',
			resource,
			target: at,
			container: container.name,
			name,
			request: request ?? '',
			limit: limit ?? '',
			current: { request, limit },
			note: `${cause.title}.`
		};
	} catch (err) {
		app.toast(errorMessage(err), 'bad');
	}
}

/** Starts a Secret or ConfigMap a container needs, with its key, on the Create page. */
export function createMissing(missing: NonNullable<Cause['missing']>, namespace: string | null) {
	const plural = missing.kind === 'Secret' ? 'secrets' : 'configmaps';
	const resource = findResource(app.resources, { group: '', version: 'v1', plural });
	if (!resource) return;
	const keys = missing.keys.length ? missing.keys : [missing.kind === 'Secret' ? 'password' : 'key'];
	app.showCreate(resource, { name: missing.name, namespace: namespace ?? undefined, data: Object.fromEntries(keys.map((k) => [k, 'change-me'])) });
}
