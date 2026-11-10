<script lang="ts">
	import { kubriUses, parseShortcut, pluginApplies, shortcutLabel } from '../k9s';
	import { backend } from '../api';
	import { FEEDBACK, feedbackLink, VERSION } from '../feedback';
	import { app } from '../state.svelte';
	import { t } from '../i18n';

	const builtin: [string, [string, string][]][] = [
		[
			t('Mouse'),
			[
				[t('Right click  or  ⋯'), t('Everything there is to do on a row')],
				[t('Double click'), t('Open, like Enter')],
				[t('Action bar'), t('Above the list, for the row under the cursor or the marked rows')],
				['‹ ›', t('Back / forward, at the top left')],
				[t('Key hints'), t('At the bottom: click one to press its key')]
			]
		],
		[
			t('Move'),
			[
				[':  or  Ctrl+K', t('Command palette: kinds, objects, actions, ctx, ns …')],
				['Ctrl+Shift+C', t('Switch context')],
				['0', t('All namespaces')],
				['j / k  or  ↑ / ↓', t('Move')],
				['g / G', t('First / last row')],
				['Ctrl+F / B  or  PgDn / PgUp', t('A page down / up')],
				['/', t('Filter: regex, !inverse, -f fuzzy, -l app=web')],
				['[ / ]  or  Alt+← / →', t('Back / forward')],
				['Backspace', t('Back; first closes the inspector')],
				['-', t('Last view, and back again')],
				['Shift+W', t('Warp to the namespace of the row')],
				['Shift+J', t('Jump to the owner')],
				['Esc', t('Close, clear selection or filter, go back')],
				['Tab', t('To the next area: sidebar, tabs, actions, list')],
				['← → ↑ ↓', t('Within tabs, actions and the sidebar, once reached with Tab')]
			]
		],
		[
			t('Look'),
			[
				['Enter', t('Open: pods of a workload, jobs of a CronJob, … (jumps.yaml)')],
				['d', t('Describe')],
				['y', t('YAML')],
				['l', t('Logs of a pod, workload or service')],
				['p', t('Logs of the previous container')],
				['m', t('Metrics from Prometheus')],
				['f', t('Maximize or restore the panel')],
				['w', t('All columns, scrolling sideways')],
				['Shift+N / A / S / P', t('Sort by name / age / status / namespace')],
				['Shift+R / T / I / O', t('Sort pods by ready / restarts / IP / node')],
				['Shift+C / M / X / Z', t('Sort by CPU / memory / %CPU / %memory')],
				['Ctrl+Z', t('Only faulty rows')],
				['Shift+E', t('Explain with your model (also findings)')],
				['c / n', t('Copy names / namespaces')],
				['Ctrl+S', t('Save the list as CSV')]
			]
		],
		[
			t('Inspector'),
			[
				['Tab', t('From the list or the overview into the inspector')],
				['← / →', t('Overview, Events, YAML, Metrics, Logs, Shell')],
				['↑ / ↓  or  PgUp / PgDn', t('Scroll; j / k and g / G too')],
				['Tab / Shift+Tab', t('Links and buttons in the inspector')],
				['Esc', t('Back to the list; Esc again closes the inspector')]
			]
		],
		[
			t('Change'),
			[
				['Space', t('Mark the row')],
				['Shift+F10', t("The row's menu, like a right click")],
				['Ctrl+Space', t('Mark up to here')],
				['Ctrl+A / Ctrl+\\', t('Mark all / none')],
				['e', t('Edit YAML (Ctrl+S saves)')],
				['s', t('Scale')],
				['r', t('Rollout restart')],
				['Shift+L', t('Labels and annotations')],
				['Ctrl+D  or  Delete', t('Delete')],
				[':new [kind name --image …]', t('Create from a template or YAML, like kubectl create')]
			]
		],
		[
			t('Tools'),
			[
				[':pulses  or  :overview', t('Overview: health, warnings, CPU and memory')],
				[':xray deploy [ns]', t('Topology as trees: what each object depends on')],
				[':graph [ns]', t('Topology as one graph, with Ingress request paths')],
				[':changes [ns] [1h]', t('What changed: rollouts, scaling, config edits and warnings')],
				[':helm', t('Helm releases')],
				[':pf', t('Port-forwards')],
				[':rbac  or  :access', t('Access: who may do what')],
				[':popeye  or  :checks', t('Checks: common mistakes, scored')],
				[':rightsizing', t('Requests far from what containers use')],
				[':bench', t('HTTP benchmarks')],
				[':tools', t('Every tool on one page')],
				[':kinds', t('Every kind the cluster serves')],
				[':fleet', t('Every context at once, with its health')],
				[':search web ctx:*', t('Objects by name, label and field, in one context or several')],
				[':integrations', t('Prometheus, Grafana, Loki, Flux, Argo CD, cert-manager')]
			]
		],
		[
			t('App'),
			[
				['Ctrl+`  or  :term', t('Terminal with kubectl and helm set to this context (Shift: another one)')],
				[':snapshot [ns|all]', t('Save a namespace or the cluster to a file to open offline, or open one')],
				[':kubeconfig', t('The kubeconfig files Kubri reads, and load more')],
				[':config', t('Settings, with k9s aliases, hotkeys, plugins and skins')],
				[':skin name', t('Switch skin; :skin kubri for the built-in colors')],
				['Ctrl+ +  /  −  /  0', t('Zoom in, out, back to 100% (:appearance for colors)')],
				['?', t('This help')],
				[':feedback [idea]', t('Report a problem or suggest an idea on GitHub, with your version filled in')]
			]
		],
		[
			t('YAML editor'),
			[
				['Ctrl+S  or  Ctrl+Enter', t('Review the change and save')],
				['Esc', t('Close the search, keep one cursor, then stop editing')],
				['Ctrl+F / Ctrl+H', t('Find / replace')],
				['F3 / Shift+F3', t('Next / previous match')],
				['Ctrl+G', t('Go to line')],
				['Ctrl+D', t('Delete the line')],
				['Ctrl+Shift+D', t('Duplicate the line')],
				['Alt+↑ / ↓', t('Move the line up or down')],
				['Ctrl+/', t('Comment or uncomment the line')],
				['Alt+J', t('Also select the next occurrence')],
				['Ctrl+Shift+L', t('Select every occurrence')],
				['Ctrl+Alt+↑ / ↓', t('Another cursor above or below')],
				[t('Ctrl+click  or  Alt+drag'), t('Another cursor, or a block of them')],
				['Ctrl+Space', t('Suggest fields')]
			]
		],
		[
			t('Logs'),
			[
				['/', t('Search: text, /regex/ or fields like level>=warn status>=500; Esc clears')],
				['n / N', t('Next / previous match; Enter in the search too')],
				['0 – 6', t('Tail, all, 1m, 5m, 15m, 30m, 1h')],
				['t / w', t('Timestamps / wrap')],
				['s', t('Autoscroll')],
				['m', t('Mark: a line across the log')],
				['x', t('Explain the selected lines, or the last 200')],
				['c / Ctrl+S', t('Copy / save')],
				['Shift+C', t('Clear')]
			]
		],
		[
			t('Pods'),
			[
				['s', t('Shell')],
				['a', t('Attach')],
				['Shift+D', t('Debug a pod, or a shell on a node')],
				['o', t('Show the node')],
				['i', t('Set image (also workloads)')],
				['z', t('Delete completed and failed pods')],
				['Shift+F', t('Port-forward (also deployments, services)')]
			]
		],
		[
			t('Port-forwards'),
			[
				['Shift+F', t('Start a port-forward')],
				['b', t('HTTP benchmark')],
				['o', t('Open in the browser')],
				['c  or  Enter', t('Copy URL')],
				['s', t('Favorite: start whenever Kubri connects')],
				['Ctrl+D', t('Stop')]
			]
		],
		[
			t('Workloads'),
			[
				['Enter', t('Pods (Deployments, StatefulSets, DaemonSets, Jobs, Services)')],
				['z', t('ReplicaSets of a Deployment')],
				['Ctrl+L', t('Roll back to a ReplicaSet')],
				['t', t('Run a CronJob now')],
				['s', t('Suspend or resume a CronJob')],
				['x', t('Show the values of a Secret')],
				['u', t('Use a namespace; what uses a ConfigMap, Secret, PVC or ServiceAccount')]
			]
		],
		[
			t('Nodes'),
			[
				['c', t('Cordon')],
				['u', t('Uncordon')],
				['r', t('Drain')]
			]
		],
		[
			t('Topology'),
			[
				['h / l  or  ← / →', t('Collapse / expand')],
				['Space', t('Toggle')],
				['Enter  or  d', t('Describe')],
				['/', t('Filter')]
			]
		],
		[
			t('Helm'),
			[
				['Enter  or  n', t('Overview and notes')],
				['v / m / h', t('Values / manifest / history')],
				['c / x', t('Compare revisions / drift from the cluster')],
				['u', t('Upgrade with changed values')],
				['i', t('Install a chart')],
				['r', t('Roll back (previous or shown revision)')],
				['Ctrl+D', t('Uninstall')],
				['Esc', t('Back to the releases')]
			]
		],
		[
			t('Checks'),
			[
				['h / l', t('Collapse / expand a kind')],
				['Enter  or  d', t('Describe the object')],
				['Shift+R', t('Scan again')]
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

	const shared = t(' (where the view does not use the key)');

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
		if (hotkeys.length) all.push([t('k9s hotkeys'), hotkeys]);
		if (plugins.length) all.push([t('k9s plugins here'), plugins]);
		return all;
	});

	let box = $state<HTMLDivElement>();
	let search = $state<HTMLInputElement>();
	let filter = $state('');

	/** The groups with only the keys that match the filter, in the key or in what it does. */
	const shown = $derived.by(() => {
		const query = filter.trim().toLowerCase();
		if (!query) return groups;
		return groups
			.map(([title, keys]): [string, [string, string][]] => [
				title,
				keys.filter(([key, what]) => key.toLowerCase().includes(query) || what.toLowerCase().includes(query) || title.toLowerCase().includes(query))
			])
			.filter(([, keys]) => keys.length > 0);
	});

	// The overlay takes the focus while open and gives it back when it closes.
	$effect(() => {
		const previous = document.activeElement as HTMLElement | null;
		(search ?? box)?.focus();
		return () => {
			if (previous?.isConnected) previous.focus();
		};
	});

	function onkeydown(e: KeyboardEvent) {
		if (e.key === 'Escape' && filter) {
			e.preventDefault();
			filter = '';
		} else if (e.key === 'Escape' || (e.key === '?' && e.target !== search)) {
			e.preventDefault();
			app.helpOpen = false;
		} else if (e.key === 'Tab') {
			// Only the filter takes the focus, so it stays there instead of going to the page behind.
			e.preventDefault();
			search?.focus();
		}
	}
</script>

<svelte:window {onkeydown} />

<div class="backdrop" onclick={() => (app.helpOpen = false)} role="presentation"></div>
<div class="help" role="dialog" aria-modal="true" aria-label={t('Keyboard shortcuts')} tabindex="-1" bind:this={box}>
	<h2>
		{t('Keyboard shortcuts')}
		<button
			class="tour-link"
			onclick={() => {
				app.helpOpen = false;
				app.tourOpen = true;
			}}>{t('Show the quick tour')}</button
		>
	</h2>
	<input
		class="filter"
		bind:this={search}
		bind:value={filter}
		placeholder={t('Type to find a key or a command…')}
		aria-label={t('Filter the shortcuts')}
		spellcheck="false"
	/>
	<div class="groups">
		{#each shown as [title, keys] (title)}
			<section>
				<h3>{title}</h3>
				<dl>
					{#each keys as [key, what], i (key + i)}
						<dt><kbd>{key.replaceAll('  or  ', `  ${t('or')}  `)}</kbd></dt>
						<dd>{what}</dd>
					{/each}
				</dl>
			</section>
		{:else}
			<p class="none">{t('Nothing matches “{filter}”.', { filter })}</p>
		{/each}
	</div>
	<p class="feedback">
		Kubri {VERSION} ·
		{t('Something wrong or missing?')}
		<a href={feedbackLink('bug', backend.desktop)} target="_blank" rel="noreferrer">{FEEDBACK.bug}</a>
		·
		<a href={feedbackLink('idea', backend.desktop)} target="_blank" rel="noreferrer">{FEEDBACK.idea}</a>
	</p>
</div>

<style>
	h2 {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
	}
	.tour-link {
		border: none;
		background: none;
		color: var(--accent);
		font-size: 12.5px;
		font-weight: 400;
	}
	.filter {
		width: 100%;
		margin-bottom: 4px;
		padding: 7px 10px;
		font-size: 13px;
	}
	.none {
		color: var(--text-muted);
		font-size: 13px;
	}
	.feedback {
		margin: 10px 0 0;
		padding-top: 10px;
		border-top: 1px solid var(--border-subtle);
		color: var(--text-muted);
		font-size: 12.5px;
	}
	.feedback a {
		color: var(--accent);
	}
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
	/* Groups flow down the columns, so a short one leaves no gap beside a long one. */
	.groups {
		columns: 2 300px;
		column-gap: 28px;
	}
	section {
		break-inside: avoid;
		padding: 12px 0 6px;
	}
	h3 {
		margin: 0 0 8px;
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
