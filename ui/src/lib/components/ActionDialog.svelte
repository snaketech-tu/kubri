<script lang="ts">
	import { tick, untrack } from 'svelte';
	import { deleteObjects, launchPlugin, patchMetadata, restart, scale, startPortForward } from '../actions';
	import { parseHeaders } from '../bench';
	import { backend, type DeleteOptions, type DrainReport } from '../api';
	import { errorMessage } from '../format';
	import { forwardablePorts, suggestedLocalPort, type ForwardablePort } from '../pods';
	import { app, type Dialog } from '../state.svelte';

	let { dialog }: { dialog: Dialog } = $props();

	let busy = $state(false);
	let error = $state<string | null>(null);
	let confirmButton = $state<HTMLButtonElement>();
	let firstField = $state<HTMLInputElement | HTMLTextAreaElement>();

	// Delete
	let force = $state(false);
	let propagation = $state<NonNullable<DeleteOptions['propagation']>>('background');

	// Scale
	let replicas = $state<number | null>(null);

	// Labels and annotations, one key=value per line
	let labelsText = $state('');
	let annotationsText = $state('');
	let original = { labels: {} as Record<string, string>, annotations: {} as Record<string, string> };

	// Port-forward
	let ports = $state<ForwardablePort[] | null>(null);
	let remotePort = $state<number | null>(null);
	let localPort = $state<number | null>(null);
	let address = $state('127.0.0.1');

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

	// Drain
	let ignoreDaemonsets = $state(true);
	let deleteEmptydirData = $state(false);
	let forceDrain = $state(false);
	let gracePeriod = $state<number | null>(null);
	let report = $state<DrainReport | null>(null);

	function pickPort(port: number) {
		remotePort = port;
		localPort = suggestedLocalPort(port);
	}

	$effect(() => {
		if (dialog.kind === 'benchmark') {
			queueMicrotask(() => firstField?.focus());
		} else if (dialog.kind === 'scale' || dialog.kind === 'labels' || dialog.kind === 'portforward') {
			backend
				.getObject(dialog.target)
				.then((obj) => {
					if (dialog.kind === 'scale') {
						replicas = typeof obj.spec?.replicas === 'number' ? obj.spec.replicas : 1;
					} else if (dialog.kind === 'portforward') {
						ports = forwardablePorts(obj);
						if (ports.length) pickPort(ports[0].port);
					} else {
						original = { labels: obj.metadata.labels ?? {}, annotations: obj.metadata.annotations ?? {} };
						labelsText = toLines(original.labels);
						annotationsText = toLines(original.annotations);
					}
					queueMicrotask(() => firstField?.focus());
				})
				.catch((err) => (error = errorMessage(err)));
		} else {
			queueMicrotask(() => confirmButton?.focus());
		}
	});

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
			if (i <= 0) throw new Error(`Expected key=value, got "${trimmed}"`);
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
		app.dialog = null;
	}

	async function confirm() {
		busy = true;
		error = null;
		let ok = false;
		try {
			switch (dialog.kind) {
				case 'delete':
					ok = await deleteObjects(dialog.resource, dialog.targets, {
						force,
						gracePeriodSeconds: force ? 0 : null,
						propagation
					});
					break;
				case 'scale':
					if (replicas === null || replicas < 0 || !Number.isInteger(replicas)) {
						throw new Error('Replicas must be a whole number of 0 or more');
					}
					ok = await scale(dialog.target, replicas);
					break;
				case 'restart':
					ok = await restart(dialog.target);
					break;
				case 'labels':
					ok = await patchMetadata(
						dialog.target,
						diff(original.labels, fromLines(labelsText)),
						diff(original.annotations, fromLines(annotationsText))
					);
					break;
				case 'portforward':
					if (!remotePort || remotePort < 1 || remotePort > 65535) throw new Error('Pick a port between 1 and 65535');
					if (localPort !== null && (localPort < 0 || localPort > 65535)) throw new Error('Invalid local port');
					ok = await startPortForward({
						target: dialog.target,
						remotePort,
						localPort: localPort || null,
						address: address.trim() || null
					});
					break;
				case 'benchmark': {
					const whole = (n: number | null, what: string, max: number) => {
						if (n === null || !Number.isInteger(n) || n < 1 || n > max) throw new Error(`${what} must be a whole number from 1 to ${max}`);
						return n;
					};
					const run = await backend.startBenchmark({
						forwardId: dialog.forward.id,
						method,
						path: path.trim() || '/',
						headers: parseHeaders(headersText),
						body: hasBody && body ? body : null,
						concurrency: whole(concurrency, 'Concurrency', 200),
						requests: byDuration ? null : whole(requests, 'Requests', 1_000_000),
						durationSecs: byDuration ? whole(seconds, 'Duration', 600) : null,
						timeoutSecs: whole(timeout, 'Timeout', 120)
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
					// Closed first: a terminal plugin opens over the window.
					const { plugin, run } = dialog;
					close();
					await launchPlugin(plugin, run);
					return;
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
					if (report.failed.length === 0) app.toast(`Drained ${dialog.target.name}: ${pods(report.evicted.length)} evicted`);
					break;
			}
		} catch (err) {
			error = errorMessage(err);
		}
		busy = false;
		if (ok) close();
		// The button was disabled while working, which moved focus out of the dialog.
		else tick().then(() => confirmButton?.focus());
	}

	function onkeydown(e: KeyboardEvent) {
		if (e.key === 'Escape') {
			e.preventDefault();
			close();
		} else if (e.key === 'Enter' && (e.ctrlKey || e.metaKey || !(e.target instanceof HTMLTextAreaElement))) {
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
		app.toast(output.trim().split('\n').pop() || 'Done');
	}

	function pods(n: number) {
		return `${n} ${n === 1 ? 'pod' : 'pods'}`;
	}

	function name(t: { namespace: string | null; name: string }) {
		return t.namespace ? `${t.namespace}/${t.name}` : t.name;
	}

	const title = $derived.by(() => {
		switch (dialog.kind) {
			case 'delete':
				return `Delete ${dialog.resource.kind}`;
			case 'scale':
				return `Scale ${dialog.resource.kind}`;
			case 'restart':
				return `Restart ${dialog.resource.kind}`;
			case 'labels':
				return 'Labels and annotations';
			case 'portforward':
				return 'Port-forward';
			case 'drain':
				return 'Drain node';
			case 'benchmark':
				return 'HTTP benchmark';
			case 'helmRollback':
				return 'Roll back release';
			case 'helmUninstall':
				return 'Uninstall release';
			case 'plugin':
				return dialog.plugin.description || dialog.plugin.name;
		}
	});
	const hasBody = $derived(['POST', 'PUT', 'PATCH'].includes(method));
	const danger = $derived(
		dialog.kind === 'delete' ||
			dialog.kind === 'helmUninstall' ||
			(dialog.kind === 'drain' && !report) ||
			(dialog.kind === 'plugin' && dialog.plugin.dangerous)
	);
</script>

<div class="backdrop" onclick={close} role="presentation"></div>
<div class="dialog" role="dialog" aria-modal="true" aria-label={title} tabindex="-1" {onkeydown}>
	<h2>{title}</h2>

	{#if dialog.kind === 'delete'}
		<p>
			{dialog.targets.length === 1 ? 'This deletes' : `This deletes ${dialog.targets.length} objects:`}
		</p>
		<ul class="targets">
			{#each dialog.targets.slice(0, 6) as t (name(t))}
				<li>{name(t)}</li>
			{/each}
			{#if dialog.targets.length > 6}<li class="muted">and {dialog.targets.length - 6} more</li>{/if}
		</ul>
		<label class="row"><input type="checkbox" bind:checked={force} /> Force (no grace period)</label>
		<label class="row">
			Dependents
			<select bind:value={propagation}>
				<option value="background">delete in background</option>
				<option value="foreground">delete first (foreground)</option>
				<option value="orphan">keep (orphan)</option>
			</select>
		</label>
	{:else if dialog.kind === 'scale'}
		<p>Set the number of replicas for <strong>{name(dialog.target)}</strong>.</p>
		<input
			class="replicas"
			type="number"
			min="0"
			bind:value={replicas}
			bind:this={firstField}
			disabled={replicas === null}
		/>
	{:else if dialog.kind === 'restart'}
		<p>Restart all pods of <strong>{name(dialog.target)}</strong> with a rolling update?</p>
	{:else if dialog.kind === 'labels'}
		<p class="muted">One <code>key=value</code> per line. Remove a line to delete it.</p>
		<label class="field">
			Labels
			<textarea rows="5" bind:value={labelsText} bind:this={firstField} spellcheck="false"></textarea>
		</label>
		<label class="field">
			Annotations
			<textarea rows="5" bind:value={annotationsText} spellcheck="false"></textarea>
		</label>
	{:else if dialog.kind === 'portforward'}
		<p>Forward a local port to <strong>{name(dialog.target)}</strong>.</p>
		{#if ports === null}
			<p class="muted">Loading ports…</p>
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
			<p class="muted">No ports are declared; enter one.</p>
		{/if}
		<div class="grid">
			<label class="field">
				{dialog.resource.kind === 'Service' ? 'Service port' : 'Container port'}
				<input type="number" min="1" max="65535" bind:value={remotePort} bind:this={firstField} />
			</label>
			<label class="field">
				Local port
				<input type="number" min="0" max="65535" bind:value={localPort} placeholder="any free port" />
			</label>
			<label class="field">
				Address
				<input bind:value={address} placeholder="127.0.0.1" />
			</label>
		</div>
		{#if address.trim() && !['127.0.0.1', 'localhost', '::1'].includes(address.trim())}
			<p class="warn">Other machines on your network will be able to reach this port.</p>
		{/if}
	{:else if dialog.kind === 'drain'}
		{#if report}
			{#if report.failed.length && !report.evicted.length}
				<p>
					<strong>{dialog.target.name}</strong> is cordoned, but no pods were evicted because
					{report.failed.length === 1 ? 'this pod blocks' : 'these pods block'} the drain:
				</p>
			{:else}
				<p>
					Evicted {pods(report.evicted.length)} from <strong>{dialog.target.name}</strong>.
					{report.failed.length ? 'These could not be evicted:' : ''}
				</p>
			{/if}
			{#if report.failed.length}
				<ul class="issues bad">
					{#each report.failed as issue (issue.pod)}<li><code>{issue.pod}</code> {issue.reason}</li>{/each}
				</ul>
			{/if}
			{#if report.skipped.length}
				<details>
					<summary class="muted">{pods(report.skipped.length)} left running</summary>
					<ul class="issues">
						{#each report.skipped as issue (issue.pod)}<li><code>{issue.pod}</code> {issue.reason}</li>{/each}
					</ul>
				</details>
			{/if}
		{:else}
			<p>
				Cordon <strong>{dialog.target.name}</strong> and evict its pods. PodDisruptionBudgets are respected.
			</p>
			<label class="row"><input type="checkbox" bind:checked={ignoreDaemonsets} /> Ignore DaemonSet pods</label>
			<label class="row"><input type="checkbox" bind:checked={deleteEmptydirData} /> Delete emptyDir data</label>
			<label class="row">
				<input type="checkbox" bind:checked={forceDrain} /> Force: also evict pods no controller will recreate
			</label>
			<label class="row">
				Grace period
				<input class="grace" type="number" min="0" bind:value={gracePeriod} placeholder="pod default" /> seconds
			</label>
		{/if}
	{:else if dialog.kind === 'benchmark'}
		<p>
			Send requests through the port-forward to <strong>{dialog.forward.pod}</strong> on port
			{dialog.forward.podPort}, and measure how fast it answers.
		</p>
		<div class="request">
			<select bind:value={method} aria-label="Method">
				{#each ['GET', 'HEAD', 'POST', 'PUT', 'PATCH', 'DELETE', 'OPTIONS'] as m (m)}<option>{m}</option>{/each}
			</select>
			<span class="origin">localhost:{dialog.forward.localPort}</span>
			<input class="path" bind:value={path} bind:this={firstField} placeholder="/" aria-label="Path" spellcheck="false" />
		</div>
		<div class="grid">
			<label class="field">
				Concurrency
				<input type="number" min="1" max="200" bind:value={concurrency} />
			</label>
			<div class="field">
				<span class="mode" role="group" aria-label="Stop after">
					<button class:active={!byDuration} onclick={() => (byDuration = false)}>Requests</button>
					<button class:active={byDuration} onclick={() => (byDuration = true)}>Seconds</button>
				</span>
				{#if byDuration}
					<input type="number" min="1" max="600" bind:value={seconds} aria-label="Duration in seconds" />
				{:else}
					<input type="number" min="1" max="1000000" bind:value={requests} aria-label="Number of requests" />
				{/if}
			</div>
			<label class="field">
				Timeout (s)
				<input type="number" min="1" max="120" bind:value={timeout} />
			</label>
		</div>
		<label class="field">
			Headers
			<textarea rows="2" bind:value={headersText} placeholder="Host: shop.example.com" spellcheck="false"></textarea>
		</label>
		{#if hasBody}
			<label class="field">
				Body
				<textarea rows="3" bind:value={body} spellcheck="false"></textarea>
			</label>
		{/if}
		<p class="muted">Ctrl+Enter starts it. The pod gets real traffic, so mind production.</p>
	{:else if dialog.kind === 'helmRollback'}
		<p>
			Roll <strong>{name(dialog.release)}</strong> back from revision {dialog.release.revision} to revision
			{dialog.revision}? Helm records this as a new revision.
		</p>
	{:else if dialog.kind === 'helmUninstall'}
		<p>
			Uninstall <strong>{name(dialog.release)}</strong>? This deletes every object of the release and its history.
		</p>
	{:else if dialog.kind === 'plugin'}
		{@const subject = dialog.run.target ?? dialog.run.release}
		<p>
			{#if subject}
				Run the plugin <strong>{dialog.plugin.name}</strong> on <strong>{name(subject)}</strong>?
			{:else}
				Run the plugin <strong>{dialog.plugin.name}</strong>?
			{/if}
		</p>
		<pre class="command">{dialog.command}</pre>
		{#if dialog.plugin.dangerous}
			<p class="muted">Its k9s configuration marks it as dangerous.</p>
		{/if}
	{/if}

	{#if error}<p class="error">{error}</p>{/if}

	<div class="buttons">
		{#if !report}<button class="secondary" onclick={close}>Cancel</button>{/if}
		<button
			class="primary"
			class:danger
			bind:this={confirmButton}
			disabled={busy}
			onclick={confirm}
		>
			{busy
				? 'Working…'
				: report
					? 'Close'
					: {
							delete: 'Delete',
							scale: 'Scale',
							restart: 'Restart',
							labels: 'Save',
							portforward: 'Start',
							drain: 'Drain',
							benchmark: 'Start',
							helmRollback: 'Roll back',
							helmUninstall: 'Uninstall',
							plugin: 'Run'
						}[dialog.kind]}
		</button>
	</div>
</div>

<style>
	.backdrop {
		position: fixed;
		inset: 0;
		z-index: 50;
		background: rgb(0 0 0 / 35%);
	}
	.dialog {
		position: fixed;
		top: 50%;
		left: 50%;
		z-index: 51;
		width: min(480px, calc(100vw - 32px));
		max-height: 85vh;
		overflow: auto;
		padding: 20px 22px;
		transform: translate(-50%, -50%);
		border: 1px solid var(--border);
		border-radius: 12px;
		background: var(--surface-1);
		box-shadow: var(--shadow);
		outline: none;
	}
	h2 {
		margin: 0 0 12px;
		font-size: 17px;
	}
	p {
		margin: 0 0 10px;
	}
	.targets {
		margin: 0 0 14px;
		padding-left: 18px;
		font-family: var(--mono);
		font-size: 12.5px;
	}
	.row {
		display: flex;
		align-items: center;
		gap: 8px;
		margin: 8px 0;
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
	.request select {
		padding: 6px 8px;
		border: none;
		border-right: 1px solid var(--border);
		border-radius: 0;
		background: var(--surface-2);
		font-family: var(--mono);
		font-size: 12.5px;
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
		width: 110px;
	}
	.warn {
		color: var(--warn);
		font-size: 13px;
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
