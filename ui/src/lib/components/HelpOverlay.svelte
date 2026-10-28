<script lang="ts">
	import { kubriUses, parseShortcut, pluginApplies, shortcutLabel } from '../k9s';
	import { app } from '../state.svelte';

	const builtin: [string, [string, string][]][] = [
		[
			'Navigate',
			[
				[':  or  Ctrl+K', 'Command palette (pods, deploy, ctx, ns …)'],
				['Ctrl+Shift+C', 'Switch context'],
				['j / k  or  ↑ / ↓', 'Move'],
				['g / G', 'First / last row'],
				['/', 'Filter (-l key=value for labels)'],
				['0', 'All namespaces'],
				[':pulses', 'Cluster overview'],
				[':xray deploy [ns]', 'XRay: what each object depends on'],
				[':rbac', 'Who may do what'],
				[':helm', 'Helm releases'],
				[':popeye', 'Sanitizer: problems, scored'],
				[':bench', 'HTTP benchmarks'],
				[':pf', 'Port-forwards'],
				[':config', 'k9s aliases, hotkeys, plugins and skins'],
				[':skin name', 'Switch skin; :skin kubri for the built-in colors'],
				['w', 'Wide columns'],
				['Shift+N / Shift+A', 'Sort by name / age'],
				['Esc', 'Close, clear selection or filter']
			]
		],
		[
			'Inspect',
			[
				['Enter  or  d', 'Describe'],
				['y', 'YAML'],
				['e', 'Edit YAML (Ctrl+S saves)'],
				['l', 'Logs of a pod, workload or service'],
				['p', 'Logs of the previous container'],
				['f', 'Maximize or restore the panel']
			]
		],
		[
			'Act',
			[
				['Space', 'Select row'],
				['Ctrl+A', 'Select all'],
				['Ctrl+D  or  Delete', 'Delete'],
				['s', 'Scale'],
				['r', 'Rollout restart'],
				['Shift+L', 'Labels and annotations'],
				['?', 'This help']
			]
		],
		[
			'Pods',
			[
				['s', 'Shell'],
				['a', 'Attach'],
				['Shift+F', 'Port-forward (also deployments, services)']
			]
		],
		[
			'Port-forwards',
			[
				['b', 'HTTP benchmark'],
				['c  or  Enter', 'Copy URL'],
				['Ctrl+D', 'Stop']
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

	function onkeydown(e: KeyboardEvent) {
		if (e.key === 'Escape' || e.key === '?') {
			e.preventDefault();
			app.helpOpen = false;
		}
	}
</script>

<svelte:window {onkeydown} />

<div class="backdrop" onclick={() => (app.helpOpen = false)} role="presentation"></div>
<div class="help" role="dialog" aria-label="Keyboard shortcuts">
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
