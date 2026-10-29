<script lang="ts">
	import type { ResourceInfo } from '../api';
	import logo from '../assets/logo.svg';
	import { NAV_SECTIONS } from '../nav';
	import { app, sameResource } from '../state.svelte';
	import { xrayResources } from '../xray';
	import Icon, { type IconName } from './Icon.svelte';

	let filter = $state('');

	const visible = $derived(
		app.resources.filter(
			(r) =>
				r.verbs.includes('watch') &&
				(r.kind.toLowerCase().includes(filter.toLowerCase()) ||
					r.plural.includes(filter.toLowerCase()) ||
					r.shortNames.includes(filter.toLowerCase()))
		)
	);
	// Resources are sorted with the core group first, so core events win over events.k8s.io.
	const sections = $derived(
		NAV_SECTIONS.map(({ title, items }) => ({
			title,
			items: items.flatMap(({ plural, label, icon }) => {
				const resource = visible.find((r) => r.plural === plural);
				return resource ? [{ resource, label, icon }] : [];
			})
		})).filter((s) => s.items.length)
	);
	const others = $derived.by(() => {
		const pinned = new Set(sections.flatMap((s) => s.items.map((i) => i.resource)));
		return visible.filter((r) => !pinned.has(r));
	});

	const running = $derived(app.benchmarks.filter((b) => b.state === 'running').length);

	function showXray() {
		const resource = app.xrayResource ?? xrayResources(app.resources)[0];
		if (resource) app.showXray(resource);
	}

	const isOpen = (r: ResourceInfo) => app.page === 'resources' && !!app.selected && sameResource(r, app.selected);
</script>

<nav aria-label="Cluster navigation">
	<div class="brand">
		<img src={logo} alt="" width="28" height="28" />
		<span>kubri</span>
	</div>
	<input class="filter" bind:value={filter} aria-label="Find a resource type" placeholder="Find resources…" />
	<div class="scroll">
		<button class:active={app.page === 'pulses'} onclick={() => app.showPulses()} title=":pulses">
			<Icon name="overview" />
			<span class="label">Overview</span>
			<span class="hint">:pulses</span>
		</button>
		<button class:active={app.page === 'create'} onclick={() => app.showCreate()} title=":new">
			<Icon name="plus" />
			<span class="label">Create</span>
			<span class="hint">:new</span>
		</button>
		{#each sections as section (section.title)}
			<h3>{section.title}</h3>
			{#each section.items as { resource, label, icon } (resource.plural)}
				{@render item(resource, label, icon)}
			{/each}
		{/each}

		<h3>Explore</h3>
		<button class:active={app.page === 'xray'} onclick={showXray} title=":xray deploy">
			<Icon name="topology" />
			<span class="label">Topology</span>
			<span class="hint">XRay</span>
		</button>
		<button class:active={app.page === 'helm'} onclick={() => app.showHelm()} title=":helm">
			<Icon name="helm" />
			<span class="label">Helm</span>
		</button>
		<button class:active={app.page === 'rbac'} onclick={() => app.showRbac()} title=":rbac">
			<Icon name="access" />
			<span class="label">Access</span>
			<span class="hint">RBAC</span>
		</button>
		<button class:active={app.page === 'sanitizer'} onclick={() => app.showSanitizer()} title=":popeye">
			<Icon name="sanitizer" />
			<span class="label">Sanitizer</span>
		</button>
		<button class:active={app.page === 'integrations'} onclick={() => app.showIntegrations()} title=":integrations">
			<Icon name="plug" />
			<span class="label">Integrations</span>
		</button>
		<button class:active={app.page === 'portforwards'} onclick={() => app.showPortForwards()} title=":pf">
			<Icon name="forward" />
			<span class="label">Port-forwards</span>
			{#if app.forwards.length}<span class="badge">{app.forwards.length}</span>{/if}
		</button>
		<button class:active={app.page === 'benchmarks'} onclick={() => app.showBenchmarks()} title=":bench">
			<Icon name="benchmark" />
			<span class="label">Benchmarks</span>
			{#if running}<span class="badge" title="Running">{running}</span>{/if}
		</button>
		<button class:active={app.page === 'config'} onclick={() => app.showConfig()} title=":config">
			<Icon name="settings" />
			<span class="label">Settings</span>
		</button>
		{#if others.length}
			<h3>All resources</h3>
			{#each others as r (r.group + '/' + r.version + '/' + r.plural)}
				{@render item(r, r.kind, 'resource', r.group)}
			{/each}
		{/if}
	</div>
	<div class="bottom">
		<div class="keys">
			<strong><Icon name="terminal" size={16} /> Your keys. Still here.</strong>
			<span>Use <kbd>:</kbd> or <kbd>Ctrl K</kbd> to jump anywhere.</span>
		</div>
		<button class="help" onclick={() => (app.helpOpen = true)} title="Keys and commands (?)">
			<Icon name="book" />
			<span class="label">Shortcuts & help</span>
		</button>
	</div>
</nav>

{#snippet item(r: ResourceInfo, label: string, icon: IconName, group = '')}
	<button
		class:active={isOpen(r)}
		aria-current={isOpen(r) ? 'page' : undefined}
		onclick={() => app.open(r)}
		title={r.shortNames.length ? `:${r.shortNames[0]}` : `:${r.plural}`}
	>
		<Icon name={icon} />
		<span class="label">{label}</span>
		{#if group}<span class="hint">{group}</span>{/if}
	</button>
{/snippet}

<style>
	nav {
		display: flex;
		flex-direction: column;
		min-height: 0;
		border-right: 1px solid var(--border);
		background: var(--surface-1);
	}
	.brand {
		display: flex;
		align-items: center;
		gap: 10px;
		height: 64px;
		flex: none;
		padding: 0 22px;
		font-size: 26px;
		font-weight: 700;
		letter-spacing: -0.045em;
	}
	.filter {
		margin: 6px 14px 8px;
	}
	.scroll {
		flex: 1;
		overflow-y: auto;
		padding: 4px 14px 12px;
	}
	h3 {
		margin: 18px 12px 6px;
		font-size: 10.5px;
		font-weight: 650;
		letter-spacing: 0.1em;
		text-transform: uppercase;
		color: var(--text-muted);
	}
	button {
		display: flex;
		align-items: center;
		gap: 11px;
		width: 100%;
		padding: 8px 12px;
		border: none;
		border-radius: 7px;
		background: none;
		color: var(--text-muted);
		font-size: 13px;
		text-align: left;
	}
	button:hover {
		background: var(--surface-2);
		color: var(--text);
	}
	button.active {
		background: color-mix(in srgb, var(--accent) 13%, var(--surface-1));
		color: var(--accent-strong);
		font-weight: 650;
	}
	.label {
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.hint {
		margin-left: auto;
		min-width: 0;
		font-size: 10.5px;
		font-weight: 400;
		color: var(--text-muted);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.badge {
		margin-left: auto;
		padding: 0 7px;
		border-radius: 999px;
		background: var(--accent);
		color: var(--on-accent);
		font-size: 11px;
		font-weight: 600;
	}
	.bottom {
		flex: none;
		padding: 10px 14px 14px;
		border-top: 1px solid var(--border-subtle);
	}
	.keys {
		display: grid;
		gap: 4px;
		margin-bottom: 8px;
		padding: 10px 12px;
		border: 1px solid var(--border);
		border-radius: 8px;
		font-size: 11.5px;
		color: var(--text-muted);
	}
	.keys strong {
		display: flex;
		align-items: center;
		gap: 7px;
		color: var(--text);
		font-size: 12px;
	}
	kbd {
		padding: 0 4px;
		border: 1px solid var(--border);
		border-radius: 4px;
		font-family: var(--mono);
		font-size: 10.5px;
	}
	.help {
		color: var(--accent-strong);
		font-weight: 600;
	}

	/* Short windows keep the list and drop the reminder. */
	@media (max-height: 1000px) {
		.keys {
			display: none;
		}
	}
	@media (max-width: 720px) {
		nav {
			max-height: 150px;
			border-right: 0;
			border-bottom: 1px solid var(--border);
		}
		.brand,
		.bottom,
		h3,
		.hint {
			display: none;
		}
		.filter {
			margin: 8px 12px;
		}
		.scroll {
			display: flex;
			flex-wrap: wrap;
			gap: 4px;
			padding: 0 12px 8px;
		}
		button {
			width: auto;
			padding: 6px 10px;
		}
	}
</style>
