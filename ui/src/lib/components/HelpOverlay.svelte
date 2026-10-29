<script lang="ts">
	import { kubriUses, parseShortcut, pluginApplies, shortcutLabel } from '../k9s';
	import { app } from '../state.svelte';

	const builtin: [string, [string, string][]][] = [
		[
			'Mouse',
			[
				['Right click  or  ⋯', 'Everything there is to do on a row'],
				['Double click', 'Open, like Enter'],
				['Action bar', 'Above the list, for the row under the cursor or the marked rows'],
				['‹ ›', 'Back / forward, at the top left'],
				['Key hints', 'At the bottom: click one to press its key']
			]
		],
		[
			'Navigate',
			[
				[':  or  Ctrl+K', 'Command palette (pods, deploy, ctx, ns …)'],
				['Ctrl+Shift+C', 'Switch context'],
				['j / k  or  ↑ / ↓', 'Move'],
				['g / G', 'First / last row'],
				['/', 'Filter: regex, !inverse, -f fuzzy, -l app=web'],
				['0', 'All namespaces'],
				['[ / ]  or  Alt+← / →', 'Back / forward'],
				['-', 'Last view, and back again'],
				['Shift+W', 'Warp to the namespace of the row'],
				['Shift+J', 'Jump to the owner'],
				[':pulses', 'Cluster overview'],
				[':xray deploy [ns]', 'XRay: what each object depends on'],
				[':graph [ns]', 'XRay graph: how objects connect, and Ingress request paths'],
				[':rbac', 'Who may do what'],
				[':helm', 'Helm releases'],
				[':new [kind name --image …]', 'Create from a template or YAML, like kubectl create'],
				[':popeye', 'Sanitizer: problems, scored'],
				[':integrations', 'Prometheus, Grafana, Loki, Flux, Argo CD, cert-manager'],
				[':rightsizing', 'Requests far from what containers use'],
				[':fleet', 'Every context at once, with its health'],
				[':search web ctx:*', 'Objects by name, label and field, in one context or several'],
				[':bench', 'HTTP benchmarks'],
				[':pf', 'Port-forwards'],
				[':config', 'k9s aliases, hotkeys, plugins and skins'],
				[':skin name', 'Switch skin; :skin kubri for the built-in colors'],
				['w', 'Wide columns'],
				['Shift+N / A / S / P', 'Sort by name / age / status / namespace'],
				['Shift+R / T / I / O', 'Sort pods by ready / restarts / IP / node'],
				['Shift+C / M / X / Z', 'Sort by CPU / memory / %CPU / %memory'],
				['Ctrl+Z', 'Only faulty rows'],
				['Esc', 'Close, clear selection or filter, go back']
			]
		],
		[
			'Inspect',
			[
				['Enter', 'Open: pods of a workload, jobs of a CronJob, … (jumps.yaml)'],
				['d', 'Describe'],
				['y', 'YAML'],
				['m', 'Metrics from Prometheus'],
				['e', 'Edit YAML (Ctrl+S saves)'],
				['l', 'Logs of a pod, workload or service'],
				['p', 'Logs of the previous container'],
				['f', 'Maximize or restore the panel'],
				['c / n', 'Copy names / namespaces'],
				['Ctrl+S', 'Save the list as CSV']
			]
		],
		[
			'Logs',
			[
				['/', 'Search: text, /regex/ or fields like level>=warn status>=500; Esc clears'],
				['n / N', 'Next / previous match; Enter in the search too'],
				['0 – 6', 'Tail, all, 1m, 5m, 15m, 30m, 1h'],
				['t / w', 'Timestamps / wrap'],
				['s', 'Autoscroll'],
				['m', 'Mark: a line across the log'],
				['x', 'Explain the selected lines, or the last 200'],
				['c / Ctrl+S', 'Copy / save'],
				['Shift+C', 'Clear']
			]
		],
		[
			'Act',
			[
				['Space', 'Select row'],
				['Ctrl+Space', 'Select up to here'],
				['Ctrl+A / Ctrl+\\', 'Select all / none'],
				['Ctrl+D  or  Delete', 'Delete'],
				['s', 'Scale'],
				['r', 'Rollout restart'],
				['Shift+L', 'Labels and annotations'],
				['Shift+E', 'Explain with your model (also findings)'],
				['?', 'This help']
			]
		],
		[
			'Pods',
			[
				['s', 'Shell'],
				['a', 'Attach'],
				['Shift+D', 'Debug a pod, or a shell on a node'],
				['o', 'Show the node'],
				['i', 'Set image (also workloads)'],
				['z', 'Delete completed and failed pods'],
				['Shift+F', 'Port-forward (also deployments, services)']
			]
		],
		[
			'Port-forwards',
			[
				['b', 'HTTP benchmark'],
				['o', 'Open in the browser'],
				['c  or  Enter', 'Copy URL'],
				['s', 'Favorite: start whenever Kubri connects'],
				['Ctrl+D', 'Stop']
			]
		],
		[
			'Workloads',
			[
				['Enter', 'Pods (Deployments, StatefulSets, DaemonSets, Jobs, Services)'],
				['z', 'ReplicaSets of a Deployment'],
				['Ctrl+L', 'Roll back to a ReplicaSet'],
				['t', 'Run a CronJob now'],
				['s', 'Suspend or resume a CronJob'],
				['x', 'Show the values of a Secret'],
				['u', 'Use a namespace; what uses a ConfigMap, Secret, PVC or ServiceAccount']
			]
		],
		[
			'Nodes',
			[
				['c', 'Cordon'],
				['u', 'Uncordon'],
				['r', 'Drain']
			]
		],
		[
			'XRay',
			[
				['h / l  or  ← / →', 'Collapse / expand'],
				['Space', 'Toggle'],
				['Enter  or  d', 'Describe'],
				['/', 'Filter']
			]
		],
		[
			'Helm',
			[
				['Enter  or  n', 'Overview and notes'],
				['v / m / h', 'Values / manifest / history'],
				['c / x', 'Compare revisions / drift from the cluster'],
				['u', 'Upgrade with changed values'],
				['i', 'Install a chart'],
				['r', 'Roll back (previous or shown revision)'],
				['Ctrl+D', 'Uninstall'],
				['Esc', 'Back to the releases']
			]
		],
		[
			'Sanitizer',
			[
				['h / l', 'Collapse / expand a kind'],
				['Enter  or  d', 'Describe the object'],
				['Shift+R', 'Scan again']
			]
		]
	];

	/** k9s keys as listed in the help, like `Ctrl+L`, and whether Kubri may keep the key. */
	function keyName(text: string, override: boolean) {
		const shortcut = parseShortcut(text);
		if (!shortcut) return null;
		const name = shortcutLabel(shortcut).replace('^', 'Ctrl+').replace('⌥', 'Alt+').replace('⇧', 'Shift+');
		return { name, shared: !override && kubriUses(shortcut) };
	}

	const shared = ' (where the view does not use the key)';

	// The k9s hotkeys, and the plugins of the view the help was opened from.
	const groups = $derived.by(() => {
		const all = [...builtin];
		const hotkeys = (app.k9s?.hotkeys ?? []).flatMap((h): [string, string][] => {
			const key = keyName(h.shortCut, h.override);
			return key ? [[key.name, (h.description || `:${h.command}`) + (key.shared ? shared : '')]] : [];
		});
		const plugins = (app.k9s?.plugins ?? [])
			.filter((p) => pluginApplies(p, app.pluginView))
			.flatMap((p): [string, string][] => {
				const key = keyName(p.shortCut, p.override);
				return key ? [[key.name, (p.description || p.name) + (key.shared ? shared : '')]] : [];
			});
		if (hotkeys.length) all.push(['k9s hotkeys', hotkeys]);
		if (plugins.length) all.push(['k9s plugins here', plugins]);
		return all;
	});

	let box = $state<HTMLDivElement>();

	// The overlay takes the focus while open and gives it back when it closes.
	$effect(() => {
		const previous = document.activeElement as HTMLElement | null;
		box?.focus();
		return () => {
			if (previous?.isConnected) previous.focus();
		};
	});

	function onkeydown(e: KeyboardEvent) {
		if (e.key === 'Escape' || e.key === '?') {
			e.preventDefault();
			app.helpOpen = false;
		} else if (e.key === 'Tab') {
			// Nothing inside takes the focus, so it stays on the overlay instead of the page behind.
			e.preventDefault();
			box?.focus();
		}
	}
</script>

<svelte:window {onkeydown} />

<div class="backdrop" onclick={() => (app.helpOpen = false)} role="presentation"></div>
<div class="help" role="dialog" aria-modal="true" aria-label="Keyboard shortcuts" tabindex="-1" bind:this={box}>
	<h2>Keyboard shortcuts</h2>
	<div class="groups">
		{#each groups as [title, keys] (title)}
			<section>
				<h3>{title}</h3>
				<dl>
					{#each keys as [key, what], i (key + i)}
						<dt><kbd>{key}</kbd></dt>
						<dd>{what}</dd>
					{/each}
				</dl>
			</section>
		{/each}
	</div>
</div>

<style>
	.backdrop {
		position: fixed;
		inset: 0;
		z-index: 50;
		background: rgb(0 0 0 / 35%);
	}
	.help:focus {
		outline: none;
	}
	.help {
		position: fixed;
		top: 50%;
		left: 50%;
		z-index: 51;
		width: min(860px, calc(100vw - 32px));
		max-height: 85vh;
		overflow: auto;
		padding: 20px 24px;
		transform: translate(-50%, -50%);
		border: 1px solid var(--border);
		border-radius: 12px;
		background: var(--surface-1);
		box-shadow: var(--shadow);
	}
	h2 {
		margin: 0 0 8px;
		font-size: 17px;
	}
	.groups {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(260px, 1fr));
		gap: 8px 28px;
	}
	h3 {
		margin: 14px 0 8px;
		font-size: 11px;
		letter-spacing: 0.06em;
		text-transform: uppercase;
		color: var(--text-muted);
	}
	dl {
		display: grid;
		grid-template-columns: max-content 1fr;
		gap: 6px 12px;
		margin: 0;
		font-size: 13px;
	}
	dd {
		margin: 0;
	}
	kbd {
		padding: 1px 6px;
		border: 1px solid var(--border);
		border-radius: 4px;
		background: var(--surface-2);
		font-family: var(--mono);
		font-size: 11.5px;
		white-space: nowrap;
	}
</style>
