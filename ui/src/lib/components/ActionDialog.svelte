<script lang="ts">
	import { lightDismiss } from '../dismiss';
	import { tick, untrack } from 'svelte';
	import {
		copyLines,
		deleteObjects,
		launchPlugin,
		renewCertificate,
		patchMetadata,
		pluginCommand,
		restart,
		rollback,
		runGitOps,
		scale,
		setImages,
		startPortForward,
		suspendCronJob,
		triggerCronJob
	} from '../actions';
	import { parseHeaders } from '../bench';
	import { backend, type Change, type ContainerImage, type DeleteOptions, type DrainReport, type Preview } from '../api';
	import { helmCommand, kubectlCommand } from '../kubectl';
	import DiffView from './DiffView.svelte';
	import HelmCompare from './HelmCompare.svelte';
	import ManagedNote from './ManagedNote.svelte';
	import Select from './Select.svelte';
	import { errorMessage } from '../format';
	import { inputError, inputVariables } from '../k9s';
	import {
		PORT_FORWARDS_KEY,
		annotatedForwards,
		containerImages,
		forwardablePorts,
		suggestedLocalPort,
		type ForwardablePort
	} from '../pods';
	import { autoscalerFor, type Autoscaler } from '../autoscale';
	import { validReplicas } from '../forms';
	import { app, refOf, type Dialog } from '../state.svelte';
	import { hasUsers, isPods } from '../pods';
	import { summarizeUsers } from '../references';
	import type { Reference } from '../api';
	import { plainOptions } from '../select';
	import { plural, t } from '../i18n';
	import Phrase from './Phrase.svelte';

	let { dialog }: { dialog: Exclude<Dialog, { kind: 'forbidden' | 'kubectl' | 'debug' | 'compare' | 'snapshot' | 'grafana' }> } = $props();

	let busy = $state(false);

	/** Whether Argo CD's auto-sync is on now, for the action that turns it off or on. */
	let autoSync = $state<{ on: boolean; options: string[] } | null>(null);
	$effect(() => {
		if (dialog.kind !== 'gitops' || dialog.action !== 'autoSync') return;
		const target = dialog.target;
		backend.getObject(target).then(
			(app) => {
				const automated = (app.spec as { syncPolicy?: { automated?: Record<string, unknown> | null } } | undefined)?.syncPolicy?.automated;
				const on = !!automated && automated.enabled !== false;
				autoSync = { on, options: on ? ['selfHeal', 'prune'].filter((k) => automated![k]) : [] };
			},
			(err) => (error = errorMessage(err))
		);
	});
	let error = $state<string | null>(null);
	let confirmButton = $state<HTMLButtonElement>();
	let firstField = $state<HTMLInputElement | HTMLTextAreaElement>();
	let box = $state<HTMLDialogElement>();
	let cancelButton = $state<HTMLButtonElement>();
	let confirmInput = $state<HTMLInputElement>();
	/** The context the targets were reviewed in; the dialog refuses to run in another. */
	const context = untrack(() => app.cluster?.context);
	/** What the user typed to confirm a bulk or production action. */
	let confirmation = $state('');
	// A modal dialog keeps the focus inside; closing it returns the focus to where it was.
	$effect(() => {
		const previous = document.activeElement as HTMLElement | null;
		box?.showModal();
		return () => previous?.focus();
	});

	// Delete
	let force = $state(false);
	let propagation = $state<NonNullable<DeleteOptions['propagation']>>('background');
	/** What uses the objects about to be deleted, which breaks without them; null while looking. */
	let users = $state<{ target: string; refs: Reference[] }[] | null>(null);
	$effect(() => {
		if (dialog.kind !== 'delete' || !hasUsers(dialog.resource)) return;
		const targets = dialog.targets.slice(0, 20);
		let current = true;
		Promise.all(
			targets.map((t) =>
				backend
					.usedBy(t)
					.then((refs) => ({ target: name(t), refs }))
					// Not knowing is no reason to block a delete.
					.catch(() => ({ target: name(t), refs: [] as Reference[] }))
			)
		).then((found) => current && (users = found.filter((u) => u.refs.length)));
		return () => (current = false);
	});

	// Scale
	let replicas = $state<number | null>(null);
	/** True until the object's replicas are in; an emptied field is null as well, but stays editable. */
	let loadingReplicas = $state(true);
	/** What a scale starts from, and the autoscaler that would undo it. */
	let currentReplicas = $state<number | null>(null);
	let autoscaler = $state<Autoscaler | null>(null);

	// Labels and annotations, one key=value per line
	let labelsText = $state('');
	let annotationsText = $state('');
	let original = { labels: {} as Record<string, string>, annotations: {} as Record<string, string> };

	// Port-forward
	let ports = $state<ForwardablePort[] | null>(null);
	let remotePort = $state<number | null>(null);
	let localPort = $state<number | null>(null);
	// k9s `portForwardAddress`, or K9S_DEFAULT_PF_ADDRESS.
	let address = $state(untrack(() => app.k9s?.portForwardAddress) ?? '127.0.0.1');
	let keepForward = $state(false);

	// Image: the containers as they are, and the images typed in.
	let images = $state<ContainerImage[] | null>(null);
	let newImages = $state<Record<string, string>>({});

	// Plugin inputs, by name, starting with their defaults: off for a switch, the first choice
	// of a dropdown that needs one.
	let inputs = $state<Record<string, string>>(
		untrack(() =>
			dialog.kind === 'plugin'
				? Object.fromEntries(
						dialog.plugin.inputs.map((i) => [
							i.name,
							i.default ||
								(i.type === 'bool' ? 'false' : i.type === 'dropdown' && i.required ? (i.options[0] ?? '') : '')
						])
					)
				: {}
		)
	);

	// Benchmark: the run to repeat, or the defaults k9s uses (one worker, 200 requests).
	// A dialog is opened afresh for each run, so its first value is the one to fill in.
	const preset = untrack(() => (dialog.kind === 'benchmark' ? dialog.preset : undefined));
	let method = $state(preset?.method ?? 'GET');
	let path = $state(preset?.path ?? '/');
	let concurrency = $state<number | null>(preset?.concurrency ?? 1);
	let byDuration = $state(!!preset?.durationSecs);
	let requests = $state<number | null>(preset?.requests ?? 200);
	let seconds = $state<number | null>(preset?.durationSecs ?? 30);
	let timeout = $state<number | null>(preset?.timeoutSecs ?? 20);
	let headersText = $state(
		Object.entries(preset?.headers ?? {})
			.map(([k, v]) => `${k}: ${v}`)
			.join('\n')
	);
	let body = $state(preset?.body ?? '');

	// Memory limit
	let memory = $state(untrack(() => (dialog.kind === 'memory' ? dialog.fix.suggested : '')));
	// Right-sizing: one resource's request and limit, prefilled with the advice.
	let sizeRequest = $state(untrack(() => (dialog.kind === 'resources' ? dialog.request : '')));
	let sizeLimit = $state(untrack(() => (dialog.kind === 'resources' ? dialog.limit : '')));
	/** The requests and limits that change, as typed. */
	function sizeChange() {
		if (dialog.kind !== 'resources') return null;
		const changed = (value: string, now: string | null) => (value.trim() && value.trim() !== now ? { [dialog.name]: value.trim() } : {});
		const requests = changed(sizeRequest, dialog.current.request);
		const limits = changed(sizeLimit, dialog.current.limit);
		return Object.keys(requests).length || Object.keys(limits).length ? { requests, limits } : null;
	}

	// Drain
	let ignoreDaemonsets = $state(true);
	let deleteEmptydirData = $state(false);
	let forceDrain = $state(false);
	let gracePeriod = $state<number | null>(null);
	let report = $state<DrainReport | null>(null);

	function pickPort(port: number, local = suggestedLocalPort(port)) {
		remotePort = port;
		localPort = local;
	}

	$effect(() => {
		if (dialog.kind === 'benchmark' || dialog.kind === 'memory' || dialog.kind === 'resources') {
			queueMicrotask(() => firstField?.focus());
		} else if (dialog.kind === 'plugin' && dialog.plugin.inputs.length) {
			queueMicrotask(focusFirstField);
		} else if (dialog.kind === 'scaleMany') {
			// The first one's count, to start from.
			loadingReplicas = true;
			backend
				.getObject(dialog.targets[0])
				.then((obj) => (replicas = typeof obj.spec?.replicas === 'number' ? obj.spec.replicas : 1))
				.catch(() => (replicas = 1))
				.finally(() => {
					loadingReplicas = false;
					void tick().then(() => {
						firstField?.focus();
						if (firstField instanceof HTMLInputElement) firstField.select();
					});
				});
		} else if (
			dialog.kind === 'scale' ||
			dialog.kind === 'labels' ||
			dialog.kind === 'portforward' ||
			dialog.kind === 'image'
		) {
			if (dialog.kind === 'scale') loadingReplicas = true;
			backend
				.getObject(dialog.target)
				.then((obj) => {
					if (dialog.kind === 'scale') {
						replicas = typeof obj.spec?.replicas === 'number' ? obj.spec.replicas : 1;
						currentReplicas = replicas;
						autoscaler = null;
						const scaled = dialog.target;
						const hpa = app.resources.find(
							(r) => r.group === 'autoscaling' && r.plural === 'horizontalpodautoscalers'
						);
						autoscalerFor(scaled, dialog.resource.kind, hpa && refOf(hpa)).then((found) => {
							if (dialog.kind === 'scale' && dialog.target === scaled) autoscaler = found;
						});
					} else if (dialog.kind === 'portforward') {
						ports = forwardablePorts(obj);
						// The port the `k9scli.io/port-forwards` annotation names comes first, as in k9s.
						const [wanted] = annotatedForwards(obj, PORT_FORWARDS_KEY);
						if (wanted) pickPort(wanted.remote, wanted.local ?? suggestedLocalPort(wanted.remote));
						else if (ports.length) pickPort(ports[0].port);
					} else if (dialog.kind === 'image') {
						images = containerImages(obj);
						newImages = Object.fromEntries(images.map((c) => [c.name, c.image]));
						queueMicrotask(focusFirstField);
						return;
					} else {
						original = { labels: obj.metadata.labels ?? {}, annotations: obj.metadata.annotations ?? {} };
						labelsText = toLines(original.labels);
						annotationsText = toLines(original.annotations);
					}
					// The replicas field is disabled while it loads: enable it before focusing it.
					loadingReplicas = false;
					void tick().then(() => {
						firstField?.focus();
						if (firstField instanceof HTMLInputElement && firstField.type === 'number') firstField.select();
					});
				})
				.catch((err) => (error = errorMessage(err)))
				.finally(() => (loadingReplicas = false));
		} else {
			// Where a word must be typed first, its field; Enter there does nothing until it matches.
			queueMicrotask(() => (danger ? (confirmInput ?? cancelButton)?.focus() : confirmButton?.focus()));
		}
	});

	/** For dialogs whose fields come from the object or the plugin. */
	function focusFirstField() {
		box?.querySelector<HTMLElement>('input, select, textarea')?.focus();
	}

	function toLines(map: Record<string, string>) {
		return Object.entries(map)
			.filter(([k]) => k !== 'kubectl.kubernetes.io/last-applied-configuration')
			.map(([k, v]) => `${k}=${v}`)
			.join('\n');
	}

	function fromLines(text: string): Record<string, string> {
		const out: Record<string, string> = {};
		for (const line of text.split('\n')) {
			const trimmed = line.trim();
			if (!trimmed) continue;
			const i = trimmed.indexOf('=');
			if (i <= 0) throw new Error(t('Expected key=value, got “{line}”', { line: trimmed }));
			out[trimmed.slice(0, i).trim()] = trimmed.slice(i + 1).trim();
		}
		return out;
	}

	/** A merge patch that sets changed keys and removes deleted ones. */
	function diff(before: Record<string, string>, after: Record<string, string>) {
		const patch: Record<string, string | null> = {};
		for (const key of Object.keys(before)) {
			if (!(key in after) && key !== 'kubectl.kubernetes.io/last-applied-configuration') patch[key] = null;
		}
		for (const [key, value] of Object.entries(after)) {
			if (before[key] !== value) patch[key] = value;
		}
		return patch;
	}

	function close() {
		if (busy) return;
		app.dialog = null;
	}

	async function confirm() {
		// Enter pressed twice must not start a second port-forward or delete twice.
		if (busy) return;
		if (context !== app.cluster?.context || app.connecting) {
			error = t('The context changed. Close this dialog and review the targets again.');
			return;
		}
		if (!confirmed) return;
		// The field says what is missing; Enter in an empty one does nothing.
		if ((dialog.kind === 'scale' || dialog.kind === 'scaleMany') && !validReplicas(replicas)) return;
		busy = true;
		error = null;
		let ok = false;
		try {
			// First the dry run, where the settings ask for one; Apply then runs it for real.
			const change = previewChange();
			if (change && 'target' in dialog && JSON.stringify(change) !== previewed) {
				preview = await backend.preview(dialog.target, change);
				previewed = JSON.stringify(change);
				return;
			}
			switch (dialog.kind) {
				case 'delete':
					ok = await deleteObjects(dialog.resource, dialog.targets.map((t) => ({ ...t, context })), {
						force,
						gracePeriodSeconds: force ? 0 : null,
						propagation
					});
					break;
				case 'scale':
					if (!validReplicas(replicas)) {
						throw new Error(t('Replicas must be a whole number of 0 or more'));
					}
					ok = await scale(dialog.target, replicas);
					break;
				case 'restart':
					ok = await restart(dialog.target);
					break;
				case 'scaleMany':
				case 'restartMany': {
					const many = dialog;
					const count = replicas;
					if (many.kind === 'scaleMany' && !validReplicas(count)) {
						throw new Error(t('Replicas must be a whole number of 0 or more'));
					}
					const results = await Promise.allSettled(
						many.targets.map((t) =>
							many.kind === 'scaleMany' ? backend.scale({ ...t, context }, count!) : backend.restart({ ...t, context })
						)
					);
					const failed = results.filter((r): r is PromiseRejectedResult => r.status === 'rejected');
					const done = many.targets.length - failed.length;
					if (failed.length) error = t('{failed} of {n} failed: {error}', { failed: failed.length, n: many.targets.length, error: errorMessage(failed[0].reason) });
					if (done) {
						app.toast(
							many.kind === 'scaleMany'
								? plural(done, 'Scaled {n} {kind} object to {count}', 'Scaled {n} {kind} objects to {count}', { kind: many.resource.kind, count: count! })
								: plural(done, 'Restarting {n} {kind} object', 'Restarting {n} {kind} objects', { kind: many.resource.kind })
						);
					}
					ok = !failed.length;
					break;
				}
				case 'labels':
					ok = await patchMetadata(
						dialog.target,
						diff(original.labels, fromLines(labelsText)),
						diff(original.annotations, fromLines(annotationsText))
					);
					break;
				case 'portforward':
					if (!remotePort || remotePort < 1 || remotePort > 65535) throw new Error(t('Pick a port between 1 and 65535'));
					if (localPort !== null && (localPort < 0 || localPort > 65535)) throw new Error(t('Invalid local port'));
					ok = await startPortForward({
						target: dialog.target,
						remotePort,
						localPort: localPort || null,
						address: address.trim() || null
					}, keepForward);
					break;
				case 'benchmark': {
					const whole = (n: number | null, what: string, max: number) => {
						if (n === null || !Number.isInteger(n) || n < 1 || n > max) throw new Error(t('{what} must be a whole number from 1 to {max}', { what, max }));
						return n;
					};
					const run = await backend.startBenchmark({
						forwardId: dialog.forward.id,
						method,
						path: path.trim() || '/',
						headers: parseHeaders(headersText),
						body: hasBody && body ? body : null,
						concurrency: whole(concurrency, t('Concurrency'), 200),
						requests: byDuration ? null : whole(requests, t('Requests'), 1_000_000),
						durationSecs: byDuration ? whole(seconds, t('Duration'), 600) : null,
						timeoutSecs: whole(timeout, t('Timeout'), 120)
					});
					app.showBenchmarks(run.id);
					ok = true;
					break;
				}
				case 'helmRollback':
					helmOutput(await backend.helmRollback(dialog.release.namespace, dialog.release.name, dialog.revision));
					dialog.done();
					ok = true;
					break;
				case 'helmUninstall':
					helmOutput(await backend.helmUninstall(dialog.release.namespace, dialog.release.name));
					dialog.done();
					ok = true;
					break;
				case 'plugin': {
					const { plugin, run } = dialog;
					const invalid = inputError(plugin, inputs);
					if (invalid) throw new Error(invalid);
					// Closed first: a terminal plugin opens over the window.
					close();
					await launchPlugin(plugin, plugin.inputs.length ? { ...run, inputs: { ...inputs } } : run);
					return;
				}
				case 'trigger':
					ok = await triggerCronJob(dialog.target);
					break;
				case 'suspend':
					ok = await suspendCronJob(dialog.target, dialog.suspend);
					break;
				case 'renew':
					ok = await renewCertificate(dialog.target);
					break;
				case 'gitops': {
					const action = dialog.action === 'autoSync' ? (autoSync?.on ? 'autoSyncOff' : 'autoSyncOn') : dialog.action;
					ok = await runGitOps(dialog.target, action);
					break;
				}
				case 'rollback':
					ok = await rollback(dialog.target);
					break;
				case 'resources': {
					const change = sizeChange();
					if (!change) throw new Error(t('Nothing changes'));
					await backend.setResources(dialog.target, dialog.container, change.requests, change.limits);
					const where = { container: dialog.container, name: dialog.target.name };
					app.toast(
						dialog.name === 'cpu'
							? t('Set the CPU of {container} in {name}; new pods start with it', where)
							: t('Set the memory of {container} in {name}; new pods start with it', where)
					);
					ok = true;
					break;
				}
				case 'memory':
					await backend.setMemoryLimit(dialog.target, dialog.fix.container, memory.trim());
					app.toast(
						t('Set the memory limit of {container} in {name} to {memory}; new pods start with it', {
							container: dialog.fix.container,
							name: dialog.target.name,
							memory: memory.trim()
						})
					);
					ok = true;
					break;
				case 'image': {
					const changed = changedImages();
					if (changed.some((c) => !c.image)) throw new Error(t('An image cannot be empty'));
					if (!changed.length) throw new Error(t('Change an image first'));
					ok = await setImages(dialog.target, changed);
					break;
				}
				case 'drain':
					if (report) {
						ok = true;
						break;
					}
					report = await backend.drain(dialog.target.name, {
						ignoreDaemonsets,
						deleteEmptydirData,
						force: forceDrain,
						gracePeriodSeconds: gracePeriod
					});
					if (report.failed.length === 0) {
						app.toast(plural(report.evicted.length, 'Drained {name}: {n} pod evicted', 'Drained {name}: {n} pods evicted', { name: dialog.target.name }));
					}
					break;
			}
		} catch (err) {
			error = errorMessage(err);
		} finally {
			busy = false;
		}
		if (ok) close();
		// The button was disabled while working, which moved focus out of the dialog.
		else tick().then(() => confirmButton?.focus());
	}

	function changedImages(): ContainerImage[] {
		return (images ?? [])
			.map((c) => ({ ...c, image: newImages[c.name]?.trim() ?? c.image }))
			.filter((c, i) => c.image !== images?.[i].image);
	}

	/** The labels and annotations patch as typed, or null while a line does not parse. */
	function metadataPatch() {
		try {
			return {
				labels: diff(original.labels, fromLines(labelsText)),
				annotations: diff(original.annotations, fromLines(annotationsText))
			};
		} catch {
			return null;
		}
	}

	// Dry run
	/** The diff of the last dry run, and the change it was for. */
	let preview = $state<Preview | null>(null);
	let previewed = $state<string | null>(null);

	/** The change to dry-run before this action runs, when the settings ask for a diff. */
	function previewChange(): Change | null {
		const wanted = app.settings.diff;
		switch (dialog.kind) {
			case 'scale':
				return wanted.scale && validReplicas(replicas) ? { type: 'scale', replicas } : null;
			case 'restart':
				return wanted.restart ? { type: 'restart' } : null;
			case 'image': {
				const changed = changedImages();
				return wanted.image && changed.length && changed.every((c) => c.image) ? { type: 'images', images: changed } : null;
			}
			case 'labels': {
				const patch = metadataPatch();
				return wanted.labels && patch ? { type: 'patch', patch: { metadata: patch } } : null;
			}
			case 'resources': {
				// Always: the diff shows the pods will be replaced.
				const change = sizeChange();
				return change ? { type: 'resources', container: dialog.container, ...change } : null;
			}
			case 'memory':
				// Always: the diff shows the pods will be replaced.
				return memory.trim() ? { type: 'memory', container: dialog.fix.container, memory: memory.trim() } : null;
			default:
				return null;
		}
	}
	/** The dry run on screen is for the change as it is now typed. */
	const reviewing = $derived.by(() => {
		const change = previewChange();
		return !!preview && !!change && JSON.stringify(change) === previewed;
	});
	/** A dry run is due before the button runs the action. */
	const needsPreview = $derived(previewChange() !== null && !reviewing);

	/** The same as a kubectl or helm command, as the fields now say. */
	const commandLine = $derived.by(() => {
		const ctx = context;
		switch (dialog.kind) {
			case 'delete':
				return kubectlCommand(ctx, dialog.targets[0], {
					op: 'delete',
					targets: dialog.targets,
					options: { force, gracePeriodSeconds: force ? 0 : null, propagation }
				});
			case 'scale':
				return kubectlCommand(ctx, dialog.target, { op: 'scale', replicas });
			case 'restart':
				return kubectlCommand(ctx, dialog.target, { op: 'restart' });
			case 'labels': {
				const patch = metadataPatch();
				return kubectlCommand(ctx, dialog.target, { op: 'labels', ...(patch ?? {}) });
			}
			case 'portforward':
				return kubectlCommand(ctx, dialog.target, { op: 'portForward', local: localPort, remote: remotePort, address: address.trim() });
			case 'drain':
				return kubectlCommand(ctx, dialog.target, {
					op: 'drain',
					options: { ignoreDaemonsets, deleteEmptydirData, force: forceDrain, gracePeriodSeconds: gracePeriod }
				});
			case 'trigger':
				return kubectlCommand(ctx, dialog.target, { op: 'trigger' });
			case 'suspend':
				return kubectlCommand(ctx, dialog.target, { op: 'suspend', suspend: dialog.suspend });
			case 'image':
				return kubectlCommand(ctx, dialog.target, { op: 'image', images: changedImages() });
			case 'rollback':
				return kubectlCommand(ctx, dialog.target, {
					op: 'rollback',
					owner: dialog.owner ? `${dialog.owner.plural}.apps/${dialog.owner.name}` : null,
					revision: dialog.revision ? String(dialog.revision) : null
				});
			case 'memory':
				return kubectlCommand(ctx, dialog.target, { op: 'memory', container: dialog.fix.container, memory: memory.trim() });
			case 'resources': {
				const change = sizeChange() ?? { requests: {}, limits: {} };
				return kubectlCommand(ctx, dialog.target, { op: 'resources', container: dialog.container, ...change });
			}
			case 'helmRollback':
				return helmCommand(ctx, dialog.release, { op: 'rollback', revision: dialog.revision });
			case 'helmUninstall':
				return helmCommand(ctx, dialog.release, { op: 'uninstall' });
			default:
				return null;
		}
	});

	function onkeydown(e: KeyboardEvent) {
		if (e.key === 'Escape') {
			e.preventDefault();
			close();
		} else if (e.key === 'Enter' && (e.ctrlKey || e.metaKey || !(e.target instanceof HTMLTextAreaElement))) {
			// Enter confirms a destructive action only from its own button, or once the word to type
			// matches, never by accident.
			const typed = e.target === confirmInput && confirmed && !busy;
			if (danger && e.target !== confirmButton && !typed) {
				if (!(e.target instanceof HTMLButtonElement)) e.preventDefault();
				return;
			}
			if (e.target instanceof HTMLButtonElement && e.target !== confirmButton) return;
			e.preventDefault();
			confirm();
		} else {
			return;
		}
		// Once the dialog is closed, the list must not handle the same key too.
		e.stopPropagation();
	}

	/** helm prints what it did, e.g. "Rollback was a success! Happy Helming!". */
	function helmOutput(output: string) {
		app.toast(output.trim().split('\n').pop() || t('Done'));
	}

	function pods(n: number) {
		return plural(n, '{n} pod', '{n} pods');
	}

	/** What uses an object about to be deleted, and that it breaks without it. */
	function usedBy(u: { target: string; refs: Reference[] }) {
		const users = summarizeUsers(u.refs);
		const one = u.refs.length === 1;
		if (dialog.kind === 'delete' && dialog.targets.length === 1) {
			return one
				? t('It is used by {users}, which breaks or stops starting without it:', { users })
				: t('It is used by {users}, which break or stop starting without it:', { users });
		}
		return one
			? t('{name} is used by {users}, which breaks or stops starting without it:', { name: u.target, users })
			: t('{name} is used by {users}, which break or stop starting without it:', { name: u.target, users });
	}

	function name(t: { namespace: string | null; name: string }) {
		return t.namespace ? `${t.namespace}/${t.name}` : t.name;
	}

	/** Asks the model why the action failed, with the object it was about. */
	function explainError() {
		if (!error) return;
		const target =
			'target' in dialog && dialog.target && 'name' in dialog.target
				? dialog.target
				: dialog.kind === 'delete' && dialog.targets.length === 1
					? dialog.targets[0]
					: null;
		app.explaining = { kind: 'failure', action: title, target, error };
	}

	const title = $derived.by(() => {
		switch (dialog.kind) {
			case 'delete':
				return t('Delete {kind}', { kind: dialog.resource.kind });
			case 'scale':
				return t('Scale {kind}', { kind: dialog.resource.kind });
			case 'restart':
				return t('Restart {kind}', { kind: dialog.resource.kind });
			case 'scaleMany':
				return t('Scale {n} {kind} objects', { n: dialog.targets.length, kind: dialog.resource.kind });
			case 'restartMany':
				return t('Restart {n} {kind} objects', { n: dialog.targets.length, kind: dialog.resource.kind });
			case 'labels':
				return t('Labels and annotations');
			case 'portforward':
				return t('Port-forward');
			case 'drain':
				return t('Drain node');
			case 'benchmark':
				return t('HTTP benchmark');
			case 'helmRollback':
				return t('Roll back release');
			case 'helmUninstall':
				return t('Uninstall release');
			case 'plugin':
				return dialog.plugin.description || dialog.plugin.name;
			case 'trigger':
				return t('Run CronJob now');
			case 'suspend':
				return dialog.suspend ? t('Suspend CronJob') : t('Resume CronJob');
			case 'renew':
				return t('Renew certificate');
			case 'gitops': {
				const kind = dialog.resource.kind;
				return {
					autoSync: () => t('Auto-sync'),
					autoSyncOff: () => t('Auto-sync'),
					autoSyncOn: () => t('Auto-sync'),
					reconcile: () => t('Reconcile {kind}', { kind }),
					suspend: () => t('Suspend {kind}', { kind }),
					resume: () => t('Resume {kind}', { kind }),
					sync: () => t('Sync {kind}', { kind }),
					refresh: () => t('Refresh {kind}', { kind })
				}[dialog.action]();
			}
			case 'image':
				return t('Set image');
			case 'rollback':
				return t('Roll back {kind}', { kind: dialog.owner?.kind ?? 'Deployment' });
			case 'memory':
				return t('Raise memory limit');
			case 'resources':
				return dialog.name === 'cpu' ? t('Right-size CPU') : t('Right-size memory');
		}
	});
	const confirmLabel = $derived(
		dialog.kind === 'suspend'
			? dialog.suspend
				? t('Suspend')
				: t('Resume')
			: dialog.kind === 'gitops'
				? {
					autoSync: () => (autoSync?.on ? t('Turn off') : t('Turn on')),
						autoSyncOff: () => t('Turn off'),
						autoSyncOn: () => t('Turn on'),
					reconcile: () => t('Reconcile'),
					suspend: () => t('Suspend'),
					resume: () => t('Resume'),
					sync: () => t('Sync'),
					refresh: () => t('Refresh')
					}[dialog.action]()
			: {
					delete: t('Delete'),
					scale: t('Scale'),
					restart: t('Restart'),
					scaleMany: t('Scale all'),
					restartMany: t('Restart all'),
					labels: t('Save'),
					portforward: t('Start'),
					drain: t('Drain'),
					benchmark: t('Start'),
					helmRollback: t('Roll back'),
					helmUninstall: t('Uninstall'),
					plugin: t('Run'),
					trigger: t('Run'),
					renew: t('Renew'),
					image: t('Set'),
					rollback: t('Roll back'),
					memory: t('Set limit'),
					resources: t('Set')
				}[dialog.kind]
	);
	/** The plugin's command as it will run, with the inputs as typed. */
	const command = $derived(
		dialog.kind === 'plugin'
			? pluginCommand(dialog.plugin, { ...dialog.variables, ...inputVariables(dialog.plugin, inputs) }, dialog.run)
			: ''
	);
	const hasBody = $derived(['POST', 'PUT', 'PATCH'].includes(method));
	const danger = $derived(
		dialog.kind === 'delete' ||
			dialog.kind === 'helmUninstall' ||
			(dialog.kind === 'drain' && !report) ||
			(dialog.kind === 'plugin' && dialog.plugin.dangerous)
	);
	/**
	 * What to type before a destructive action: on a production context the object's name (the
	 * context's for several objects or a plugin), elsewhere "confirm" for bulk deletes. Null when
	 * the button is enough.
	 */
	const confirmWord = $derived.by(() => {
		if (!danger) return null;
		if (app.production) {
			if (dialog.kind === 'delete') return dialog.targets.length === 1 ? dialog.targets[0].name : (context ?? 'confirm');
			if (dialog.kind === 'helmUninstall') return dialog.release.name;
			if (dialog.kind === 'drain') return dialog.target.name;
			return context ?? 'confirm';
		}
		return dialog.kind === 'delete' && dialog.targets.length > 1 ? 'confirm' : null;
	});
	const needsConfirmation = $derived(confirmWord !== null);
	const confirmed = $derived(confirmWord === null || confirmation.trim() === confirmWord);
</script>

<dialog
	{@attach lightDismiss}
	class="dialog"
	class:wide={reviewing}
	aria-label={title}
	{onkeydown}
	oncancel={(e) => {
		e.preventDefault();
		close();
	}}
	bind:this={box}
>
	<h2>{title}</h2>
	<p class="action-context">
		<strong>{context ? app.contextName(context) : ''}</strong>{app.environment ? ` · ${app.environment}` : ''} ·
		{dialog.kind === 'delete' || dialog.kind === 'scaleMany' || dialog.kind === 'restartMany'
			? t('{n} selected', { n: dialog.targets.length })
			: t('Review the target before you continue')}
	</p>

	{#if dialog.kind === 'scale' || dialog.kind === 'restart' || dialog.kind === 'image' || dialog.kind === 'labels' || dialog.kind === 'memory' || dialog.kind === 'resources'}
		<ManagedNote target={dialog.target} compact />
	{:else if dialog.kind === 'delete' && dialog.targets.length === 1}
		<ManagedNote target={dialog.targets[0]} compact />
	{/if}

	{#if dialog.kind === 'delete'}
		<p>
			{dialog.targets.length === 1 ? t('This deletes') : t('This deletes {n} objects:', { n: dialog.targets.length })}
		</p>
		<ul class="targets">
			{#each dialog.targets as t (name(t))}
				<li>
					{name(t)}{#if dialog.statuses?.[name(t)]}<span class="muted"> · {dialog.statuses[name(t)]}</span>{/if}
				</li>
			{/each}
		</ul>
		{#if hasUsers(dialog.resource)}
			{#if users === null}
				<p class="muted">{dialog.targets.length === 1 ? t('Looking for what uses it…') : t('Looking for what uses them…')}</p>
			{:else if users.length}
				<div class="blast" role="alert">
					<strong>{t('Still in use.')}</strong>
					{#each users as u (u.target)}
						<p>{usedBy(u)}</p>
						<ul>
							{#each u.refs.slice(0, 8) as r (r.kind + r.namespace + r.name)}
								<li>{r.kind} <span class="mono">{r.name}</span>{r.how ? ` (${r.how})` : ''}</li>
							{/each}
							{#if u.refs.length > 8}<li class="muted">{t('and {n} more', { n: u.refs.length - 8 })}</li>{/if}
						</ul>
					{/each}
				</div>
			{/if}
		{/if}
		<label class="row">
			<input type="checkbox" bind:checked={force} />
			{isPods(dialog.resource) ? t('Force: skip the grace period; the container may still be running on its node') : t('Force: skip the grace period')}
		</label>
		<label class="row">
			{t('Objects it owns')}
			<Select
				bind:value={propagation}
				options={[
					{ value: 'background', label: t('delete them too (recommended)') },
					{ value: 'foreground', label: t('delete them first, then this one') },
					{ value: 'orphan', label: t('keep them') }
				]}
			/>
		</label>
	{:else if dialog.kind === 'scaleMany' || dialog.kind === 'restartMany'}
		<p>
			{dialog.kind === 'scaleMany'
				? t('Set the same number of replicas for each of these:')
				: t('Roll out new pods for each of these, like kubectl rollout restart:')}
		</p>
		<ul class="targets">
			{#each dialog.targets as t (name(t))}<li>{name(t)}</li>{/each}
		</ul>
		{#if dialog.kind === 'scaleMany'}
			<input
				class="replicas"
				type="number"
				min="0"
				aria-label={t('Replicas')}
				aria-invalid={!loadingReplicas && !validReplicas(replicas)}
				bind:value={replicas}
				bind:this={firstField}
				disabled={loadingReplicas}
			/>
			{#if !loadingReplicas && !validReplicas(replicas)}<p class="muted">{t('Enter a whole number of 0 or more.')}</p>{/if}
		{/if}
	{:else if dialog.kind === 'scale'}
		<p>
			<Phrase
				text={currentReplicas !== null ? t('Set the number of replicas for {name}, now {n}.', { n: currentReplicas }) : t('Set the number of replicas for {name}.')}
				strong={{ name: name(dialog.target) }}
			/>
		</p>
		{#if autoscaler}
			<div class="blast caution" role="note">
				<strong>{t('An autoscaler sets this.')}</strong>
				<p>
					<Phrase
						text={t(
							'HorizontalPodAutoscaler {name} keeps the replicas between {min} and {max} and adjusts them as the load changes, so a number you set here may not stay. To change the range, edit the autoscaler.',
							{ min: autoscaler.min, max: autoscaler.max }
						)}
						mono={{ name: autoscaler.name }}
					/>
				</p>
			</div>
		{/if}
		<input
			class="replicas"
			type="number"
			min="0"
			aria-label={t('Replicas')}
			aria-invalid={!loadingReplicas && !validReplicas(replicas)}
			bind:value={replicas}
			bind:this={firstField}
			disabled={loadingReplicas}
		/>
		{#if !loadingReplicas && !validReplicas(replicas)}<p class="muted">{t('Enter a whole number of 0 or more.')}</p>{/if}
	{:else if dialog.kind === 'restart'}
		<p><Phrase text={t('Restart all pods of {name} with a rolling update?')} strong={{ name: name(dialog.target) }} /></p>
	{:else if dialog.kind === 'labels'}
		<p class="muted"><Phrase text={t('One {pair} per line. Remove a line to delete it.')} code={{ pair: 'key=value' }} /></p>
		<label class="field">
			{t('Labels')}
			<textarea rows="5" bind:value={labelsText} bind:this={firstField} spellcheck="false"></textarea>
		</label>
		<label class="field">
			{t('Annotations')}
			<textarea rows="5" bind:value={annotationsText} spellcheck="false"></textarea>
		</label>
	{:else if dialog.kind === 'portforward'}
		<p><Phrase text={t('Forward a local port to {name}.')} strong={{ name: name(dialog.target) }} /></p>
		{#if ports === null}
			<p class="muted">{t('Loading ports…')}</p>
		{:else if ports.length}
			<div class="ports">
				{#each ports as p (p.port + p.detail)}
					<button class="port" class:active={remotePort === p.port} onclick={() => pickPort(p.port)}>
						<strong>{p.port}</strong>
						{#if p.name}<span>{p.name}</span>{/if}
						<span class="muted">{p.detail}</span>
					</button>
				{/each}
			</div>
		{:else}
			<p class="muted">{t('No ports are declared; enter one.')}</p>
		{/if}
		<div class="grid">
			<label class="field">
				{dialog.resource.kind === 'Service' ? t('Service port') : t('Container port')}
				<input type="number" min="1" max="65535" bind:value={remotePort} bind:this={firstField} />
			</label>
			<label class="field">
				{t('Local port')}
				<input type="number" min="0" max="65535" bind:value={localPort} placeholder={t('any free port')} />
			</label>
			<label class="field">
				{t('Address')}
				<input bind:value={address} placeholder="127.0.0.1" />
			</label>
		</div>
		{#if address.trim() && !['127.0.0.1', 'localhost', '::1'].includes(address.trim())}
			<p class="warn">{t('Other machines on your network will be able to reach this port.')}</p>
		{/if}
		<label class="row" title={t('Start it again with one click under Port-forwards in the sidebar')}>
			<input type="checkbox" bind:checked={keepForward} />
			{t('Keep as a favorite')}
		</label>
		{#if !backend.desktop}
			<p class="muted">{t('The port opens on the computer kubri-server runs on.')}</p>
		{/if}
	{:else if dialog.kind === 'drain'}
		{#if report}
			{#if report.failed.length && !report.evicted.length}
				<p>
					<Phrase
						text={report.failed.length === 1
							? t('{name} is cordoned, but no pods were evicted because this pod blocks the drain:')
							: t('{name} is cordoned, but no pods were evicted because these pods block the drain:')}
						strong={{ name: dialog.target.name }}
					/>
				</p>
			{:else}
				<p>
					<Phrase text={t('Evicted {pods} from {name}.', { pods: pods(report.evicted.length) })} strong={{ name: dialog.target.name }} />
					{report.failed.length ? t('These could not be evicted:') : ''}
				</p>
			{/if}
			{#if report.failed.length}
				<ul class="issues bad">
					{#each report.failed as issue (issue.pod)}<li><code>{issue.pod}</code> {issue.reason}</li>{/each}
				</ul>
			{/if}
			{#if report.skipped.length}
				<details>
					<summary class="muted">{plural(report.skipped.length, '{n} pod left running', '{n} pods left running')}</summary>
					<ul class="issues">
						{#each report.skipped as issue (issue.pod)}<li><code>{issue.pod}</code> {issue.reason}</li>{/each}
					</ul>
				</details>
			{/if}
		{:else}
			<p>
				<Phrase text={t('Cordon {name} and evict its pods. PodDisruptionBudgets are respected.')} strong={{ name: dialog.target.name }} />
			</p>
			<label class="row"><input type="checkbox" bind:checked={ignoreDaemonsets} /> {t('Ignore DaemonSet pods')}</label>
			<label class="row"><input type="checkbox" bind:checked={deleteEmptydirData} /> {t('Delete emptyDir data')}</label>
			<label class="row">
				<input type="checkbox" bind:checked={forceDrain} /> {t('Force: also evict pods no controller will recreate')}
			</label>
			<label class="row">
				{t('Grace period')}
				<input class="grace" type="number" min="0" bind:value={gracePeriod} placeholder={t('pod default')} />
				{t('seconds')}
			</label>
		{/if}
	{:else if dialog.kind === 'benchmark'}
		<p>
			<Phrase
				text={t('Send requests through the port-forward to {pod} on port {port}, and measure how fast it answers.', { port: dialog.forward.podPort })}
				strong={{ pod: dialog.forward.pod }}
			/>
		</p>
		<div class="request">
			<Select bind:value={method} options={plainOptions(['GET', 'HEAD', 'POST', 'PUT', 'PATCH', 'DELETE', 'OPTIONS'])} aria-label={t('Method')} mono compact />
			<span class="origin">localhost:{dialog.forward.localPort}</span>
			<input class="path" bind:value={path} bind:this={firstField} placeholder="/" aria-label={t('Path')} spellcheck="false" />
		</div>
		<div class="grid">
			<label class="field">
				{t('Concurrency')}
				<input type="number" min="1" max="200" bind:value={concurrency} />
			</label>
			<div class="field">
				<span class="mode" role="group" aria-label={t('Stop after')}>
					<button class:active={!byDuration} onclick={() => (byDuration = false)}>{t('Requests')}</button>
					<button class:active={byDuration} onclick={() => (byDuration = true)}>{t('Seconds')}</button>
				</span>
				{#if byDuration}
					<input type="number" min="1" max="600" bind:value={seconds} aria-label={t('Duration in seconds')} />
				{:else}
					<input type="number" min="1" max="1000000" bind:value={requests} aria-label={t('Number of requests')} />
				{/if}
			</div>
			<label class="field">
				{t('Timeout (s)')}
				<input type="number" min="1" max="120" bind:value={timeout} />
			</label>
		</div>
		<label class="field">
			{t('Headers')}
			<textarea rows="2" bind:value={headersText} placeholder="Host: shop.example.com" spellcheck="false"></textarea>
		</label>
		{#if hasBody}
			<label class="field">
				{t('Body')}
				<textarea rows="3" bind:value={body} spellcheck="false"></textarea>
			</label>
		{/if}
		<p class="muted">{t('Ctrl+Enter starts it. The pod gets real traffic, so mind production.')}</p>
	{:else if dialog.kind === 'helmRollback'}
		<p>
			<Phrase
				text={t('Roll {name} back from revision {from} to revision {to}? Helm records this as a new revision.', {
					from: dialog.release.revision,
					to: dialog.revision
				})}
				strong={{ name: name(dialog.release) }}
			/>
		</p>
		{#await Promise.all([backend.helmRelease(dialog.release.namespace, dialog.release.name), backend.helmRelease(dialog.release.namespace, dialog.release.name, dialog.revision)])}
			<p class="muted">{t('Comparing the revisions…')}</p>
		{:then [now, then]}
			<div class="rollback-diff">
				<HelmCompare namespace={now.namespace} before={now} after={then} />
			</div>
		{:catch err}
			<p class="muted">{t('The revisions could not be compared: {error}', { error: errorMessage(err) })}</p>
		{/await}
	{:else if dialog.kind === 'helmUninstall'}
		<p>
			<Phrase text={t('Uninstall {name}? This deletes every object of the release and its history.')} strong={{ name: name(dialog.release) }} />
		</p>
	{:else if dialog.kind === 'plugin'}
		{@const subject = dialog.run.target ?? dialog.run.release}
		<p>
			{#if subject}
				<Phrase text={t('Run the plugin {plugin} on {name}?')} strong={{ plugin: dialog.plugin.name, name: name(subject) }} />
			{:else}
				<Phrase text={t('Run the plugin {plugin}?')} strong={{ plugin: dialog.plugin.name }} />
			{/if}
		</p>
		{#each dialog.plugin.inputs as input (input.name)}
			{@const label = input.label || input.name}
			{#if input.type === 'bool'}
				<label class="row">
					<input
						type="checkbox"
						checked={inputs[input.name] === 'true'}
						onchange={(e) => (inputs[input.name] = String(e.currentTarget.checked))}
					/>
					{label}
				</label>
			{:else if input.type === 'dropdown'}
				<label class="field">
					{label}{input.required ? ' *' : ''}
					<Select
						bind:value={inputs[input.name]}
						options={[...(input.required ? [] : [{ value: '', label: t('(none)') }]), ...plainOptions(input.options)]}
					/>
				</label>
			{:else}
				<label class="field">
					{label}{input.required ? ' *' : ''}
					<input
						inputmode={input.type === 'number' ? 'decimal' : undefined}
						bind:value={inputs[input.name]}
						spellcheck="false"
					/>
				</label>
			{/if}
		{/each}
		<pre class="command">{command}</pre>
		{#if dialog.plugin.dangerous}
			<p class="muted">{t('Its k9s configuration marks it as dangerous.')}</p>
		{/if}
	{:else if dialog.kind === 'trigger'}
		<p><Phrase text={t('Run {name} now? This creates a Job from its template, as kubectl create job --from does.')} strong={{ name: name(dialog.target) }} /></p>
	{:else if dialog.kind === 'suspend'}
		{#if dialog.suspend}
			<p><Phrase text={t('Suspend {name}? It starts no new Jobs until it is resumed; running Jobs go on.')} strong={{ name: name(dialog.target) }} /></p>
		{:else}
			<p><Phrase text={t('Resume {name}? It starts Jobs on its schedule again.')} strong={{ name: name(dialog.target) }} /></p>
		{/if}
	{:else if dialog.kind === 'renew'}
		<p>
			<Phrase
				text={t(
					'Renew {name} now? cert-manager issues a new certificate from its issuer and replaces the one in its Secret, as cmctl renew does. Pods that read the Secret get the new one when they reload it.'
				)}
				strong={{ name: name(dialog.target) }}
			/>
		</p>
	{:else if dialog.kind === 'gitops'}
		{#if dialog.action === 'suspend'}
			<p>
				<Phrase
					text={t(
						'Suspend {name}? Flux stops reconciling it, so changes in its source are not applied and changes by hand are not put back, until it is resumed.'
					)}
					strong={{ name: name(dialog.target) }}
				/>
			</p>
		{:else if dialog.action === 'resume'}
			<p>
					<Phrase
						text={t('Resume {name}? Flux reconciles it now and on its interval again, putting back changes made by hand.')}
						strong={{ name: name(dialog.target) }}
					/>
				</p>
		{:else if dialog.action === 'sync'}
			<p>
				<Phrase
					text={t('Sync {name} to its target revision? Argo CD applies what Git says, running its sync hooks, as argocd app sync does.')}
					strong={{ name: name(dialog.target) }}
				/>
			</p>
		{:else if dialog.action === 'autoSync'}
			{#if !autoSync}
				<p class="muted">{t('Reading the Application…')}</p>
			{:else if autoSync.on}
				<p>
					<Phrase
						text={autoSync.options.length
							? t(
									'Auto-sync of {name} is on ({options}). Turn it off? Argo CD then applies changes from Git only when someone syncs, and leaves changes by hand alone. Kubri keeps the settings to turn it on again as it was.',
									{ options: autoSync.options.map((o) => (o === 'selfHeal' ? 'self-heal' : o)).join(', ') }
								)
							: t(
									'Auto-sync of {name} is on. Turn it off? Argo CD then applies changes from Git only when someone syncs, and leaves changes by hand alone. Kubri keeps the settings to turn it on again as it was.'
								)}
						strong={{ name: name(dialog.target) }}
					/>
				</p>
			{:else}
				<p>
					<Phrase text={t('Turn auto-sync of {name} on? Argo CD then applies changes from Git by itself.')} strong={{ name: name(dialog.target) }} />
				</p>
			{/if}
		{/if}
	{:else if dialog.kind === 'rollback'}
		{#if dialog.owner && dialog.owner.kind !== 'Deployment'}
			<p>
				<Phrase
					text={t(
						'Roll {kind} {name} back to revision {revision}? Its pods get the template saved in {source} and are replaced one by one, as kubectl rollout undo does.',
						{ kind: dialog.owner.kind, revision: dialog.revision ?? '', source: dialog.target.name }
					)}
					strong={{ name: dialog.owner.name }}
				/>
			</p>
		{:else}
			<p>
				<Phrase
					text={t('Roll the Deployment of {name} back to this ReplicaSet? Its pods get the template of {source}, as kubectl rollout undo does.', {
						source: dialog.target.name
					})}
					strong={{ name: name(dialog.target) }}
				/>
			</p>
		{/if}
	{:else if dialog.kind === 'memory'}
		<p>
			<Phrase
				text={dialog.fix.limit
					? t('{container} ran out of memory at its limit of {limit}. Set a higher limit on {object}; its pods are replaced with a rolling update.', {
							limit: dialog.fix.limit
						})
					: t('{container} ran out of memory. Set a higher limit on {object}; its pods are replaced with a rolling update.')}
				strong={{ container: dialog.fix.container, object: `${dialog.resource.kind} ${name(dialog.target)}` }}
			/>
		</p>
		<label class="field">
			<span>{t('Memory limit')}</span>
			<input class="mono" bind:value={memory} bind:this={firstField} spellcheck="false" />
		</label>
		<p class="muted">
			{dialog.fix.request ? `${t('It requests {request}.', { request: dialog.fix.request })} ` : ''}{t(
				'Kubri suggests about twice the limit. If memory keeps growing, the app may leak it, and a higher limit only buys time.'
			)}
		</p>
	{:else if dialog.kind === 'resources'}
		<p>
			<Phrase
				text={t('{container} in {object}: {note} Its pods are replaced with a rolling update.', { note: dialog.note })}
				strong={{ container: dialog.container, object: `${dialog.resource.kind} ${name(dialog.target)}` }}
			/>
		</p>
		<div class="pair">
			<label class="field">
				<span
					>{dialog.name === 'cpu' ? t('CPU request') : t('Memory request')}{dialog.current.request ? t(', now {value}', { value: dialog.current.request }) : ''}</span
				>
				<input class="mono" bind:value={sizeRequest} bind:this={firstField} spellcheck="false" placeholder={dialog.name === 'cpu' ? '100m' : '128Mi'} />
			</label>
			<label class="field">
				<span>{t('Limit')}{dialog.current.limit ? t(', now {value}', { value: dialog.current.limit }) : ''}</span>
				<input class="mono" bind:value={sizeLimit} spellcheck="false" placeholder={t('none')} />
			</label>
		</div>
	{:else if dialog.kind === 'image'}
		<p><Phrase text={t('Set the container images of {name}.')} strong={{ name: name(dialog.target) }} /></p>
		{#if images === null}
			<p class="muted">{t('Loading containers…')}</p>
		{:else}
			{#each images as c (c.name)}
				<label class="field">
					<span>{c.name}{#if c.init}<span class="muted"> (init)</span>{/if}</span>
					<input
						class="mono"
						bind:value={newImages[c.name]}
						spellcheck="false"
					/>
				</label>
			{/each}
			{#if dialog.resource.kind !== 'Pod'}
				<p class="muted">{t('The pods are replaced with a rolling update.')}</p>
			{/if}
		{/if}
	{/if}

	{#if reviewing && preview}
		<div class="preview">
			<DiffView before={preview.before} after={preview.after} maxHeight="260px" />
		</div>
	{/if}

	{#if commandLine && !report}
		<details class="command-line">
			<summary><Phrase text={t('Same as {command}')} code={{ command: commandLine.split(' ')[0] }} /></summary>
			<div class="command-row">
				<pre>{commandLine}</pre>
				<button class="copy" onclick={() => copyLines([commandLine])}>{t('Copy')}</button>
			</div>
		</details>
	{/if}

	{#if needsConfirmation}
		<label class="confirmation">
			<span>
				{#if app.production}<strong class="production">{t('Production.')}</strong>{/if}
				<Phrase text={t('Type {word} to continue')} code={{ word: confirmWord ?? '' }} />
			</span>
			<input bind:value={confirmation} bind:this={confirmInput} autocomplete="off" spellcheck="false" />
		</label>
	{/if}
	{#if error}
		<p class="error" role="alert">
			{error}
			<button class="explain" onclick={explainError} title={t('Ask your model why this failed')}>{t('Explain…')}</button>
		</p>
	{/if}

	<div class="buttons">
		{#if !report}<button class="secondary" bind:this={cancelButton} disabled={busy} onclick={close}>{t('Cancel')}</button>{/if}
		<button
			class="primary"
			class:danger
			bind:this={confirmButton}
			disabled={busy ||
				!confirmed ||
				(dialog.kind === 'gitops' && dialog.action === 'autoSync' && !autoSync) ||
				((dialog.kind === 'scale' || dialog.kind === 'scaleMany') && !validReplicas(replicas))}
			onclick={confirm}
		>
			{busy ? t('Working…') : report ? t('Close') : needsPreview ? t('Review changes') : confirmLabel}
		</button>
	</div>
</dialog>

<style>
	.rollback-diff {
		max-height: 50vh;
		overflow: auto;
	}
	.action-context {
		color: var(--text-muted);
		overflow-wrap: anywhere;
	}
	.confirmation {
		display: grid;
		gap: 8px;
		margin-top: 16px;
	}
	.preview {
		margin-top: 14px;
	}
	.command-line {
		margin-top: 12px;
		font-size: 12.5px;
		color: var(--text-muted);
	}
	.command-line summary {
		cursor: pointer;
	}
	.command-row {
		display: flex;
		align-items: flex-start;
		gap: 8px;
		margin-top: 6px;
	}
	.command-row pre {
		flex: 1;
		margin: 0;
		padding: 7px 9px;
		border: 1px solid var(--border);
		border-radius: 6px;
		background: var(--bg);
		color: var(--text);
		font-family: var(--mono);
		font-size: 12px;
		white-space: pre-wrap;
		word-break: break-all;
	}
	.copy {
		padding: 4px 9px;
		border: 1px solid var(--border);
		border-radius: 6px;
		background: var(--surface-1);
		font-size: 12px;
	}
	.confirmation .production {
		color: var(--bad);
	}
	.dialog {
		position: fixed;
		inset: 0;
		z-index: 51;
		width: min(480px, calc(100vw - 32px));
		max-height: 85dvh;
		margin: auto;
		overflow: auto;
		padding: 20px 22px;
		border: 1px solid var(--border);
		border-radius: 12px;
		background: var(--surface-1);
		box-shadow: var(--shadow);
		color: var(--text);
		outline: none;
	}
	.dialog.wide {
		width: min(760px, calc(100vw - 32px));
	}
	.dialog::backdrop {
		background: rgb(16 21 34 / 45%);
	}
	h2 {
		margin: 0 0 12px;
		font-size: 17px;
	}
	p {
		margin: 0 0 10px;
	}
	/* Every target is listed, to review before deleting; long lists scroll. */
	.targets {
		max-height: 220px;
		margin: 0 0 14px;
		padding-left: 18px;
		overflow: auto;
		font-family: var(--mono);
		font-size: 12.5px;
	}
	.row {
		display: flex;
		align-items: center;
		gap: 8px;
		margin: 8px 0;
	}
	.pair {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 12px;
	}
	.field {
		display: flex;
		flex-direction: column;
		gap: 4px;
		margin: 10px 0;
		font-size: 13px;
		color: var(--text-muted);
	}
	textarea {
		padding: 8px 10px;
		border: 1px solid var(--border);
		border-radius: 6px;
		background: var(--bg);
		color: var(--text);
		font-family: var(--mono);
		font-size: 12px;
		resize: vertical;
	}
	.replicas {
		width: 120px;
		font-size: 18px;
	}
	.ports {
		display: flex;
		flex-wrap: wrap;
		gap: 6px;
		margin-bottom: 6px;
	}
	.port {
		display: flex;
		gap: 6px;
		align-items: baseline;
		padding: 4px 10px;
		border: 1px solid var(--border);
		border-radius: 6px;
		background: var(--surface-1);
		font-size: 12.5px;
	}
	.port.active {
		border-color: var(--accent);
		background: color-mix(in srgb, var(--accent) 12%, transparent);
	}
	.grid {
		display: grid;
		grid-template-columns: 1fr 1fr 1.3fr;
		gap: 10px;
	}
	.grid input {
		width: 100%;
	}
	.request {
		display: flex;
		align-items: center;
		margin: 4px 0 6px;
		border: 1px solid var(--border);
		border-radius: 8px;
		background: var(--bg);
		overflow: hidden;
	}
	/* The method sits in the request bar like a prefix. */
	.request :global(.select) {
		align-self: stretch;
		border: none;
		border-right: 1px solid var(--border);
		border-radius: 0;
		background: var(--surface-2);
		box-shadow: none;
	}
	.request :global(.select:focus-visible),
	.request :global(.select.open) {
		background: color-mix(in srgb, var(--accent) 12%, var(--surface-2));
	}
	.origin {
		padding-left: 8px;
		color: var(--text-muted);
		font-family: var(--mono);
		font-size: 12.5px;
	}
	.path {
		flex: 1;
		min-width: 0;
		padding: 6px 8px 6px 0;
		border: none;
		background: none;
		font-family: var(--mono);
		font-size: 12.5px;
		outline: none;
	}
	.mode {
		display: flex;
		gap: 2px;
	}
	.mode button {
		padding: 0 6px;
		border: none;
		border-radius: 4px;
		background: none;
		color: var(--text-muted);
		font-size: 13px;
	}
	.mode button.active {
		background: var(--surface-2);
		color: var(--text);
		font-weight: 600;
	}
	.grace {
		width: 130px;
	}
	.warn {
		color: var(--warn);
		font-size: 13px;
	}
	.blast {
		margin: 0 0 14px;
		padding: 10px 12px;
		border: 1px solid color-mix(in srgb, var(--bad) 35%, transparent);
		border-radius: 8px;
		background: color-mix(in srgb, var(--bad) 8%, transparent);
		font-size: 13px;
	}
	.blast strong {
		color: var(--bad);
	}
	.blast.caution {
		border-color: color-mix(in srgb, var(--warn) 40%, transparent);
		background: color-mix(in srgb, var(--warn) 9%, transparent);
	}
	.blast.caution strong {
		color: var(--warn);
	}
	.blast p {
		margin: 4px 0;
	}
	.blast ul {
		margin: 0 0 4px;
		padding-left: 18px;
	}
	.issues {
		margin: 6px 0 10px;
		padding-left: 18px;
		font-size: 13px;
	}
	.issues.bad {
		color: var(--bad);
	}
	.issues code {
		font-family: var(--mono);
		font-size: 12px;
		color: var(--text);
	}
	.command {
		margin: 0 0 12px;
		padding: 8px 10px;
		border-radius: 6px;
		background: var(--surface-2);
		font-family: var(--mono);
		font-size: 12.5px;
		white-space: pre-wrap;
		word-break: break-all;
	}
	.error {
		color: var(--bad);
	}
	.error .explain {
		margin-left: 6px;
		padding: 1px 8px;
		border: 1px solid var(--border);
		border-radius: 6px;
		background: var(--surface-1);
		color: var(--text);
		font-size: 12px;
	}
	.dialog :global(.mono) {
		font-family: var(--mono);
		font-size: 12.5px;
	}
	.muted {
		color: var(--text-muted);
	}
	.buttons {
		display: flex;
		justify-content: flex-end;
		gap: 8px;
		margin-top: 18px;
	}
	.buttons button {
		padding: 7px 14px;
		border-radius: 7px;
	}
	.primary {
		border: 1px solid var(--accent);
		background: var(--accent);
		color: var(--on-accent);
	}
	.primary.danger {
		border-color: var(--bad);
		background: var(--bad);
	}
	.secondary {
		border: 1px solid var(--border);
		background: var(--surface-1);
	}
</style>
