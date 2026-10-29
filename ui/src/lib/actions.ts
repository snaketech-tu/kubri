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
	type ResourceInfo
} from './api';
import { suggest } from './commands';
import { errorMessage } from './format';
import { expandVariables, focusVariables } from './k9s';
import { AUTO_PORT_FORWARDS_KEY, annotatedForwards, suggestedLocalPort } from './pods';
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

async function runAction(success: string, action: () => Promise<unknown>): Promise<boolean> {
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
	return runAction(`Scaled ${label(target)} to ${replicas}`, () => backend.scale(target, replicas));
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
	return runAction(cordoned ? `Cordoned ${node}` : `Uncordoned ${node}`, () => backend.cordon(node, cordoned));
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
	return runAction(suspend ? `Suspended ${label(target)}` : `Resumed ${label(target)}`, () =>
		backend.suspendCronJob(target, suspend)
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

export function setImages(target: ObjectRef, images: ContainerImage[]) {
	return runAction(`Set ${images.length === 1 ? 'the image' : 'images'} of ${label(target)}`, () =>
		backend.setImages(target, images)
	);
}

/** Rolls the Deployment of a ReplicaSet back to it, like k9s Ctrl+L. */
export async function rollback(target: ObjectRef) {
	try {
		const deployment = await backend.rollback(target);
		app.toast(`Rolled Deployment ${deployment} back to ${target.name}`);
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
