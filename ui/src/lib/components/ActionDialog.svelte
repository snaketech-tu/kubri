<script lang="ts">
	import { lightDismiss } from '../dismiss';
	import { tick, untrack } from 'svelte';
	import {
		copyLines,
		deleteObjects,
		launchPlugin,
		renewCertificate,
		patchMetadata,
		restart,
		rollback,
		runGitOps,
		scale,
		setImages,
		startPortForward,
		suspendCronJob,
		triggerCronJob
	} from '../actions';
	import { backend, type Amounts, type Capacity, type ContainerImage, type DeleteOptions, type DrainReport, type Preview } from '../api';
	import DiffView from './DiffView.svelte';
	import HelmCompare from './HelmCompare.svelte';
	import ManagedNote from './ManagedNote.svelte';
	import { errorMessage } from '../format';
	import { inputError } from '../k9s';
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
	import { app, refOf } from '../state.svelte';
	import {
		actionCommand,
		confirmLabel as confirmLabelOf,
		confirmWord as confirmWordOf,
		changedImages,
		dialogTitle,
		fromLines,
		isDangerous,
		mapPatch,
		metadataPatch,
		previewChange,
		reachedNamespaces,
		targetName,
		toLines,
		type ActionDialogSpec,
		type ActionFields,
		type AutoSync
	} from '../action-dialog';
	import BenchmarkFields from './BenchmarkFields.svelte';
	import DeleteReview from './DeleteReview.svelte';
	import DrainFields from './DrainFields.svelte';
	import GitOpsReview from './GitOpsReview.svelte';
	import PluginFields from './PluginFields.svelte';
	import PortForwardFields from './PortForwardFields.svelte';
	import { plural, t } from '../i18n';
	import Phrase from './Phrase.svelte';
	import ResourcesEditor from './ResourcesEditor.svelte';
	import { applyFill, fieldProblems, initRequests, resourceChanges, rowsOf, type ResourceRow } from '../resources';

	let { dialog }: { dialog: ActionDialogSpec } = $props();

	let busy = $state(false);

	/** Whether Argo CD's auto-sync is on now, for the action that turns it off or on. */
	let autoSync = $state<AutoSync | null>(null);
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


	// Benchmark: the fields keep the run, and give it when it starts.
	let benchmark = $state<ReturnType<typeof BenchmarkFields>>();

	// Memory limit
	let memory = $state(untrack(() => (dialog.kind === 'memory' ? dialog.fix.suggested : '')));
	// Requests and limits of every container, with the room on the nodes.
	let sizeRows = $state<ResourceRow[] | null>(null);
	let sizeInit = $state<Amounts>({ cpu: 0, memory: 0 });
	let capacity = $state<Capacity | null>(null);
	let capacityError = $state<string | null>(null);
	const sizeChanges = $derived(sizeRows ? resourceChanges(sizeRows) : []);
	const sizeProblems = $derived(sizeRows ? fieldProblems(sizeRows) : []);

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
		if (dialog.kind === 'resources') {
			const { target, fill } = dialog;
			backend
				.getObject(target)
				.then((obj) => {
					const rows = rowsOf(obj);
					const field = applyFill(rows, fill);
					sizeInit = initRequests(obj);
					sizeRows = rows;
					void tick().then(() => {
						const input = box?.querySelector<HTMLInputElement>(`[data-field="${CSS.escape(field ?? '')}"]`);
						(input ?? box?.querySelector<HTMLInputElement>('input'))?.focus();
						input?.select();
					});
				})
				.catch((err) => (error = errorMessage(err)));
			backend.capacity(target).then(
				(found) => (capacity = found),
				(err) => (capacityError = errorMessage(err))
			);
		} else if (dialog.kind === 'benchmark' || dialog.kind === 'memory') {
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
						mapPatch(original.labels, fromLines(labelsText)),
						mapPatch(original.annotations, fromLines(annotationsText))
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
					if (!benchmark) break;
					const run = await backend.startBenchmark(benchmark.request());
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
					if (sizeProblems.length) throw new Error(sizeProblems[0].text);
					if (!sizeChanges.length) throw new Error(t('Nothing changes'));
					await backend.setResources(dialog.target, sizeChanges);
					app.toast(t('Set the requests and limits of {name}; new pods start with them', { name: dialog.target.name }));
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
					const changed = fields.images;
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

	// Dry run
	/** The diff of the last dry run, and the change it was for. */
	let preview = $state<Preview | null>(null);
	let previewed = $state<string | null>(null);

	/** What the fields say now, for the dry run and the command line. */
	const fields = $derived<ActionFields>({
		replicas,
		deleteOptions: { force, gracePeriodSeconds: force ? 0 : null, propagation },
		metadata: metadataPatch(original, labelsText, annotationsText),
		forward: { local: localPort, remote: remotePort, address: address.trim() },
		drain: { ignoreDaemonsets, deleteEmptydirData, force: forceDrain, gracePeriodSeconds: gracePeriod },
		images: changedImages(images, newImages),
		memory: memory.trim(),
		resources: sizeChanges,
		resourceProblems: sizeProblems.length > 0
	});
	/** The change to dry-run before this action runs, when the settings ask for a diff. */
	const change = $derived(previewChange(dialog, app.prefs.settings.diff, fields));
	/** The dry run on screen is for the change as it is now typed. */
	const reviewing = $derived(!!preview && !!change && JSON.stringify(change) === previewed);
	/** A dry run is due before the button runs the action. */
	const needsPreview = $derived(change !== null && !reviewing);

	/** The same as a kubectl or helm command, as the fields now say. */
	const commandLine = $derived(actionCommand(dialog, context, fields));

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

	const title = $derived(dialogTitle(dialog));
	const confirmLabel = $derived(confirmLabelOf(dialog, autoSync));
	const danger = $derived(isDangerous(dialog, report));
	/** The action reaches a production context, or a namespace marked as production. */
	const production = $derived(app.production || reachedNamespaces(dialog).some((ns) => app.productionIn(ns)));
	/** What to type before a destructive action, or null when the button is enough. */
	const confirmWord = $derived(confirmWordOf(dialog, danger, production, context));
	const needsConfirmation = $derived(confirmWord !== null);
	const confirmed = $derived(confirmWord === null || confirmation.trim() === confirmWord);
</script>

<dialog
	{@attach lightDismiss}
	class="dialog"
	class:wide={reviewing || dialog.kind === 'resources'}
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
		<strong>{context ? app.prefs.contextName(context) : ''}</strong>{app.environment ? ` · ${app.environment}` : ''} ·
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
		<DeleteReview resource={dialog.resource} targets={dialog.targets} statuses={dialog.statuses} bind:force bind:propagation />
	{:else if dialog.kind === 'scaleMany' || dialog.kind === 'restartMany'}
		<p>
			{dialog.kind === 'scaleMany'
				? t('Set the same number of replicas for each of these:')
				: t('Roll out new pods for each of these, like kubectl rollout restart:')}
		</p>
		<ul class="targets">
			{#each dialog.targets as t (targetName(t))}<li>{targetName(t)}</li>{/each}
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
				strong={{ name: targetName(dialog.target) }}
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
		<p><Phrase text={t('Restart all pods of {name} with a rolling update?')} strong={{ name: targetName(dialog.target) }} /></p>
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
		<PortForwardFields
			resource={dialog.resource}
			target={dialog.target}
			{ports}
			bind:remotePort
			bind:localPort
			bind:address
			bind:keep={keepForward}
			bind:firstField
		/>
	{:else if dialog.kind === 'drain'}
		<DrainFields target={dialog.target} {report} bind:ignoreDaemonsets bind:deleteEmptydirData bind:force={forceDrain} bind:gracePeriod />
	{:else if dialog.kind === 'benchmark'}
		<BenchmarkFields forward={dialog.forward} preset={dialog.preset} bind:firstField bind:this={benchmark} />
	{:else if dialog.kind === 'helmRollback'}
		<p>
			<Phrase
				text={t('Roll {name} back from revision {from} to revision {to}? Helm records this as a new revision.', {
					from: dialog.release.revision,
					to: dialog.revision
				})}
				strong={{ name: targetName(dialog.release) }}
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
			<Phrase text={t('Uninstall {name}? This deletes every object of the release and its history.')} strong={{ name: targetName(dialog.release) }} />
		</p>
	{:else if dialog.kind === 'plugin'}
		<PluginFields plugin={dialog.plugin} run={dialog.run} variables={dialog.variables} bind:inputs />
	{:else if dialog.kind === 'trigger'}
		<p><Phrase text={t('Run {name} now? This creates a Job from its template, as kubectl create job --from does.')} strong={{ name: targetName(dialog.target) }} /></p>
	{:else if dialog.kind === 'suspend'}
		{#if dialog.suspend}
			<p><Phrase text={t('Suspend {name}? It starts no new Jobs until it is resumed; running Jobs go on.')} strong={{ name: targetName(dialog.target) }} /></p>
		{:else}
			<p><Phrase text={t('Resume {name}? It starts Jobs on its schedule again.')} strong={{ name: targetName(dialog.target) }} /></p>
		{/if}
	{:else if dialog.kind === 'renew'}
		<p>
			<Phrase
				text={t(
					'Renew {name} now? cert-manager issues a new certificate from its issuer and replaces the one in its Secret, as cmctl renew does. Pods that read the Secret get the new one when they reload it.'
				)}
				strong={{ name: targetName(dialog.target) }}
			/>
		</p>
	{:else if dialog.kind === 'gitops'}
		<GitOpsReview target={dialog.target} action={dialog.action} {autoSync} />
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
					strong={{ name: targetName(dialog.target) }}
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
				strong={{ container: dialog.fix.container, object: `${dialog.resource.kind} ${targetName(dialog.target)}` }}
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
				text={t('Set what the containers of {object} request and may use at most. Its pods are replaced with a rolling update.')}
				strong={{ object: `${dialog.resource.kind} ${targetName(dialog.target)}` }}
			/>
		</p>
		{#if dialog.note}<p class="muted">{dialog.note}</p>{/if}
		{#if sizeRows === null}
			{#if !error}<p class="muted">{t('Loading containers…')}</p>{/if}
		{:else}
			<ResourcesEditor
				bind:rows={sizeRows}
				{capacity}
				{capacityError}
				init={sizeInit}
				daemon={dialog.resource.kind === 'DaemonSet'}
				problems={sizeProblems}
			/>
		{/if}
	{:else if dialog.kind === 'image'}
		<p><Phrase text={t('Set the container images of {name}.')} strong={{ name: targetName(dialog.target) }} /></p>
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
				{#if production}<strong class="production">{t('Production.')}</strong>{/if}
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
				((dialog.kind === 'scale' || dialog.kind === 'scaleMany') && !validReplicas(replicas)) ||
				(dialog.kind === 'resources' && (!sizeChanges.length || sizeProblems.length > 0))}
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
