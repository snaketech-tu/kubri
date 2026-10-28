<script lang="ts">
	import { app } from '../state.svelte';

	const groups: [string, [string, string][]][] = [
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
				[':pf', 'Port-forwards'],
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
		]
	];

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
					{#each keys as [key, what] (key)}
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
