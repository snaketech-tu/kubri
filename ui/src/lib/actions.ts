import {
	backend,
	type DeleteOptions,
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
	const focus = app.focus?.() ?? null;
	const run: PluginRun = {
		plugin: plugin.name,
		target: focus?.target ?? null,
		resource: focus?.resource ?? null,
		release: focus?.release ?? null,
		filter: focus?.filter || null,
		columns: focus?.columns ?? []
	};
	if (plugin.confirm) {
		// The variables Kubri knows here; the rest, like $CONTAINER, are filled in when it runs.
		const variables = focusVariables(focus, contextInfo());
		const command = [plugin.command, ...plugin.args]
			.map((part) => expandVariables(part, variables, run.columns))
			.join(' ');
		app.dialog = { kind: 'plugin', plugin, run, command };
		return;
	}
	launchPlugin(plugin, run);
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
