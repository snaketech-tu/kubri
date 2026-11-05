<script lang="ts">
	import type { ResourceInfo } from '../api';
	import logo from '../assets/logo.svg';
	import { version } from '../../../package.json';
	import { NAV_SECTIONS, SIDEBAR_TOOLS, TOOLS, pluralKind, toolOf } from '../nav';
	import { samePin } from '../pins';
	import { app, sameResource } from '../state.svelte';
	import Icon, { type IconName } from './Icon.svelte';

	// Resources are sorted with the core group first, so core events win over events.k8s.io.
	const sections = $derived(
		NAV_SECTIONS.map(({ title, items }) => ({
			title,
			items: items.flatMap(({ plural, label, icon }) => {
				const resource = app.resources.find((r) => r.plural === plural && r.verbs.includes('watch'));
				return resource ? [{ resource, label, icon }] : [];
			})
		})).filter((s) => s.items.length)
	);

	/** The objects opened lately in this context, without the ones already pinned. */
	const recents = $derived(
		app.recent
			.filter((r) => r.context === app.cluster?.context && r.name && !app.pinsHere.some((p) => samePin(p, r)))
			.slice(0, 3)
	);

	const running = $derived(app.benchmarks.filter((b) => b.state === 'running').length);

	const listedTools = TOOLS.slice(0, SIDEBAR_TOOLS);
	const tool = $derived(toolOf(app.page));
	/** On a tool the sidebar does not list, All tools shows where you are. */
	const otherTool = $derived(app.page === 'tools' || (tool !== null && !listedTools.some((t) => t.id === tool)));

	/** A search of its own, unless it is one saved in the sidebar. */
	const searching = $derived(app.page === 'search' && !app.settings.searches.some((s) => s.query === app.searchQuery));

	/** Marks the entry of the page shown, for screen readers as for the eye. */
	const here = (on: boolean) => (on ? ('page' as const) : undefined);

	const isOpen = (r: ResourceInfo) => app.page === 'resources' && !!app.selected && sameResource(r, app.selected);
	/** A list the sidebar does not name lights up All kinds, where it was found. */
	const otherKind = $derived(
		app.page === 'kinds' || (app.page === 'resources' && !!app.selected && !sections.some((s) => s.items.some((i) => isOpen(i.resource))))
	);

	// A shade at the bottom says there is more below, until the list is scrolled to its end.
	let scroller = $state<HTMLDivElement>();
	let items = $state<HTMLDivElement>();
	let more = $state(false);
	function measure() {
		if (scroller) more = scroller.scrollTop + scroller.clientHeight < scroller.scrollHeight - 4;
	}
	$effect(() => {
		if (!scroller || !items) return;
		const observer = new ResizeObserver(measure);
		observer.observe(scroller);
		observer.observe(items);
		return () => observer.disconnect();
	});
</script>

<nav aria-label="Cluster navigation">
	<div class="brand">
		<img src={logo} alt="" width="28" height="28" />
		<span>kubri</span>
		<span class="version" aria-label={`Version ${version}`}>v{version}</span>
	</div>
	<div class="list">
		<div class="scroll" bind:this={scroller} onscroll={measure}>
			<div class="items" bind:this={items}>
				<button class:active={app.page === 'pulses'} aria-current={here(app.page === 'pulses')} onclick={() => app.showPulses()} title="Health, warnings and use of this cluster (:pulses)">
					<Icon name="overview" />
					<span class="label">Overview</span>
					<span class="hint">:pulses</span>
				</button>
				<button class:active={app.page === 'fleet'} aria-current={here(app.page === 'fleet')} onclick={() => app.showFleet()} title="Every context at once (:fleet)">
					<Icon name="fleet" />
					<span class="label">Clusters</span>
					<span class="hint">:fleet</span>
				</button>
				<button
					class:active={searching}
					aria-current={here(searching)}
					onclick={() => app.showSearch()}
					title="Objects by name, label and field (:search)"
				>
					<Icon name="search" />
					<span class="label">Search</span>
					<span class="hint">:search</span>
				</button>
				{#each app.settings.searches as saved (saved.name)}
					<button
						class="saved"
						class:active={app.page === 'search' && app.searchQuery === saved.query}
						aria-current={here(app.page === 'search' && app.searchQuery === saved.query)}
						onclick={() => app.showSearch(saved.query)}
						title={saved.query}
					>
						<Icon name="bookmark" size={16} />
						<span class="label">{saved.name}</span>
					</button>
				{/each}
				<button class:active={app.page === 'create'} aria-current={here(app.page === 'create')} onclick={() => app.showCreate()} title="Create from a template or YAML (:new)">
					<Icon name="plus" />
					<span class="label">Create</span>
					<span class="hint">:new</span>
				</button>

				<h3>Tools</h3>
				{#each listedTools as t (t.id)}
					<button class:active={tool === t.id} aria-current={here(tool === t.id)} onclick={() => app.showTool(t.id)} title="{t.detail} ({t.command})">
						<Icon name={t.icon} />
						<span class="label">{t.label}</span>
						{#if t.id === 'portforwards' && app.forwards.length}
							<span class="badge" title="Running">{app.forwards.length}</span>
						{:else}
							<span class="hint">{t.command}</span>
						{/if}
					</button>
				{/each}
				<button class:active={otherTool} aria-current={here(otherTool)} onclick={() => app.showTools()} title="Every tool, with what it does (:tools)">
					<Icon name="tools" />
					<span class="label">All tools</span>
					{#if running}
						<span class="badge" title="Benchmarks running">{running}</span>
					{:else}
						<span class="hint">:tools</span>
					{/if}
				</button>

				{#each sections as section (section.title)}
					<h3>{section.title}</h3>
					{#each section.items as { resource, label, icon } (resource.plural)}
						{@render item(resource, label, icon)}
					{/each}
				{/each}
				<button class:active={otherKind} aria-current={here(otherKind)} onclick={() => app.showKinds()} title="Every kind the cluster serves, with a search (:kinds)">
					<Icon name="kinds" />
					<span class="label">All kinds</span>
					<span class="hint">:kinds</span>
				</button>

				{#if app.pinsHere.length}
					<h3>Pinned</h3>
					{#each app.pinsHere as pin (`${pin.group}/${pin.plural}/${pin.namespace ?? ''}/${pin.name}`)}
						<div class="pin">
							<button onclick={() => app.openPin(pin)} title={pin.name ? `${pin.kind} ${pin.namespace ? `${pin.namespace}/` : ''}${pin.name}` : `${pin.kind} list`}>
								<Icon name="bookmark" size={16} />
								<span class="label">{pin.name || pluralKind(pin.kind, pin.plural)}</span>
							</button>
							<button class="unpin" onclick={() => app.togglePin(pin)} title="Unpin" aria-label="Unpin {pin.name || pin.kind}">×</button>
						</div>
					{/each}
				{/if}
				{#if recents.length}
					<h3>Recent</h3>
					{#each recents as recent (`${recent.group}/${recent.plural}/${recent.namespace ?? ''}/${recent.name}`)}
						<button onclick={() => app.openPin(recent)} title="{recent.kind} {recent.namespace ? `${recent.namespace}/` : ''}{recent.name}">
							<Icon name="history" size={16} />
							<span class="label">{recent.name}</span>
							<span class="hint">{recent.kind}</span>
						</button>
					{/each}
				{/if}
			</div>
		</div>
		<div class="shade" class:on={more} aria-hidden="true"></div>
	</div>
	<div class="bottom">
		<button class:active={app.page === 'integrations'} aria-current={here(app.page === 'integrations')} onclick={() => app.showIntegrations()} title=":integrations">
			<Icon name="plug" />
			<span class="label">Integrations</span>
		</button>
		<button class:active={app.page === 'config'} aria-current={here(app.page === 'config')} onclick={() => app.showConfig()} title=":config">
			<Icon name="settings" />
			<span class="label">Settings</span>
		</button>
		<div class="keys">
			<strong><Icon name="terminal" size={16} /> Your keys. Still here.</strong>
			<span>Use <kbd>:</kbd> or <kbd>Ctrl K</kbd> to jump anywhere.</span>
		</div>
		<button onclick={() => (app.activityOpen = true)} title="Changes made in this session, with undo">
			<Icon name="history" />
			<span class="label">Activity</span>
			{#if app.activity.length}<span class="badge">{app.activity.length}</span>{/if}
		</button>
		<button onclick={() => (app.helpOpen = true)} title="Keys and commands (?)">
			<Icon name="book" />
			<span class="label">Shortcuts & help</span>
		</button>
	</div>
</nav>

{#snippet item(r: ResourceInfo, label: string, icon: IconName)}
	<button
		class:active={isOpen(r)}
		aria-current={isOpen(r) ? 'page' : undefined}
		onclick={() => app.open(r)}
		title={r.shortNames.length ? `:${r.shortNames[0]}` : `:${r.plural}`}
	>
		<Icon name={icon} />
		<span class="label">{label}</span>
	</button>
{/snippet}

<style>
	.pin {
		position: relative;
		display: flex;
		align-items: center;
	}
	.pin > button:first-child {
		flex: 1;
		min-width: 0;
	}
	.pin .unpin {
		position: absolute;
		right: 6px;
		width: 22px;
		padding: 0;
		opacity: 0;
		background: none;
		color: var(--text-muted);
	}
	.pin:hover .unpin,
	.pin .unpin:focus-visible {
		opacity: 1;
	}
	.pin .unpin:hover {
		color: var(--text);
	}
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
	.version {
		margin-left: auto;
		font-size: 11px;
		font-weight: 500;
		letter-spacing: 0;
		color: var(--text-muted);
	}
	.list {
		position: relative;
		display: flex;
		flex: 1;
		flex-direction: column;
		min-height: 0;
	}
	.scroll {
		flex: 1;
		min-height: 0;
		overflow-y: auto;
		padding: 4px 14px 12px;
	}
	.shade {
		position: absolute;
		right: 0;
		bottom: 0;
		left: 0;
		height: 40px;
		background: linear-gradient(to bottom, transparent, var(--surface-1));
		opacity: 0;
		transition: opacity 0.15s;
		pointer-events: none;
	}
	.shade.on {
		opacity: 1;
	}
	h3 {
		margin: 16px 12px 6px;
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
	.saved {
		padding-left: 24px;
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
		flex: 1;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	/* The command shows on hover only, so it takes no room from the label. */
	.hint {
		display: none;
		flex: none;
		max-width: 50%;
		font-size: 10.5px;
		font-weight: 400;
		color: var(--text-muted);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	@media (min-width: 1101px) {
		button:hover .hint,
		button:focus-visible .hint {
			display: inline;
		}
	}
	.badge {
		flex: none;
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

	/* Short windows keep the list and drop the reminder. */
	@media (max-height: 1000px) {
		.keys {
			display: none;
		}
	}
	/* A small laptop window keeps the labels in a narrower sidebar. */
	@media (min-width: 901px) and (max-width: 1100px) {
		.brand {
			padding: 0 16px;
		}
		.version,
		.hint {
			display: none;
		}
		.scroll,
		.bottom {
			padding-left: 8px;
			padding-right: 8px;
		}
		button {
			gap: 9px;
			padding: 7px 10px;
		}
		h3 {
			margin-left: 10px;
		}
	}
	/* Between a phone and a small laptop only the icons stay, with the labels in the tooltips. */
	@media (min-width: 721px) and (max-width: 900px) {
		.brand {
			justify-content: center;
			padding: 0;
		}
		.brand span,
		.keys,
		.label,
		.hint {
			display: none;
		}
		/* The group titles become lines between the groups. */
		h3 {
			height: 1px;
			margin: 10px 8px;
			overflow: hidden;
			background: var(--border-subtle);
			font-size: 0;
		}
		.scroll,
		.bottom {
			padding-left: 6px;
			padding-right: 6px;
		}
		button {
			justify-content: center;
			padding: 9px 0;
		}
		.saved {
			padding-left: 0;
		}
		.badge {
			position: absolute;
			margin: 0;
			transform: translate(12px, -10px);
			font-size: 10px;
			padding: 0 5px;
		}
		button {
			position: relative;
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
		.shade,
		h3,
		.hint {
			display: none;
		}
		.scroll {
			padding: 8px 12px;
		}
		.items {
			display: flex;
			flex-wrap: wrap;
			gap: 4px;
		}
		button {
			width: auto;
			padding: 6px 10px;
		}
	}
</style>
