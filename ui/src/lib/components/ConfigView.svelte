<script lang="ts">
	import { tick } from 'svelte';
	import { runCommand, runHotkey } from '../actions';
	import { isTauri } from '@tauri-apps/api/core';
	import { backend, type KindView } from '../api';
	import { ZOOMS, skinWarning, stepZoom } from '../appearance';
	import { kubriUses, parseShortcut, shortcutLabel, skinTheme, type SkinTheme } from '../k9s';
	import { forControl } from '../keys';
	import { resolveColor, type Scheme } from '../skin';
	import { app } from '../state.svelte';
	import AgentSettings from './AgentSettings.svelte';
	import ExplainSettings from './ExplainSettings.svelte';
	import ContextSettingsForm from './ContextSettingsForm.svelte';
	import PageHeader from './PageHeader.svelte';

	let previews = $state<Record<string, SkinTheme | null>>({});
	let reloading = $state(false);
	let scroller = $state<HTMLDivElement>();

	const config = $derived(app.k9s);
	const broken = $derived(config?.files.filter((f) => f.error) ?? []);

	// Each skin's colors, for the swatches.
	const requested = new Set<string>();
	$effect(() => {
		for (const name of config?.skins ?? []) {
			if (requested.has(name)) continue;
			requested.add(name);
			backend
				.k9sSkin(name)
				.then((skin) => (previews[name] = skinTheme(skin, resolveColor)))
				.catch(() => {});
		}
	});

	$effect(() => {
		const section = app.configSection;
		if (!section || !config) return;
		tick().then(() => scroller?.querySelector(`#config-${section}`)?.scrollIntoView({ block: 'start' }));
	});

	async function reload() {
		reloading = true;
		requested.clear();
		previews = {};
		await Promise.all([app.loadK9s(), app.loadViews()]);
		reloading = false;
	}

	/** What a view adds, in a few words. */
	function shows(view: KindView) {
		const count = (n: number, one: string, many: string) => (n ? [`${n} ${n === 1 ? one : many}`] : []);
		return [
			...count(view.columns.length, 'column', 'columns'),
			...(view.health.length ? ['health'] : []),
			...count(view.details.length, 'section', 'sections'),
			...count(view.actions.length, 'action', 'actions'),
			...count(view.relations.length, 'relation', 'relations')
		].join(', ');
	}

	function key(text: string) {
		const shortcut = parseShortcut(text);
		return shortcut ? { label: shortcutLabel(shortcut), taken: kubriUses(shortcut) } : null;
	}

	function onkeydown(e: KeyboardEvent) {
		if (app.overlayOpen || forControl(e)) return;
		if (e.key === 'R' && !e.ctrlKey && !e.metaKey && !e.altKey) {
			e.preventDefault();
			reload();
		}
	}

	// Kubri's own colors in light and dark, and as the system is.
	const lightTheme: SkinTheme = {
		vars: { '--bg': '#f6f7fb', '--text': '#20283d', '--accent': '#4b64f4', '--ok': '#1f9d55', '--warn': '#c27c0e', '--bad': '#d64545' },
		scheme: 'light'
	};
	const darkTheme: SkinTheme = {
		vars: { '--bg': '#101522', '--text': '#e5eaf5', '--accent': '#a6b3ff', '--ok': '#3ccf7e', '--warn': '#e5a73b', '--bad': '#f06b6b' },
		scheme: 'dark'
	};
	const kubriTheme = matchMedia('(prefers-color-scheme: dark)').matches ? darkTheme : lightTheme;
	const desktop = isTauri();
	const schemes: { value: Scheme; label: string; theme: SkinTheme }[] = [
		{ value: null, label: 'As the system', theme: kubriTheme },
		{ value: 'light', label: 'Light', theme: lightTheme },
		{ value: 'dark', label: 'Dark', theme: darkTheme }
	];
</script>

<svelte:window {onkeydown} />

<section>
	<PageHeader
		crumbs={['Explore', 'Settings']}
		title="Settings"
		subtitle="Kubri's own settings, and the aliases, hotkeys, plugins and skins from your k9s files"
	>
		{#snippet tools()}
			<button class="btn" onclick={reload} disabled={reloading} title="Read the files again (Shift+R)">
				{reloading ? 'Reading…' : 'Reload'}
			</button>
		{/snippet}
	</PageHeader>

	<div class="scroll" bind:this={scroller}>
		<div class="block">
			<h2 id="config-contexts">Contexts <span class="count">{app.contexts.length}</span></h2>
			<p class="muted">
				Names, colors and guard rails per context, kept in Kubri's settings file. The kubeconfig is never changed.
			</p>
			<div class="contexts">
				{#each app.contexts as ctx (ctx.name)}
					{@const settings = app.contextSettingsOf(ctx.name)}
					<details class="context" open={ctx.name === app.cluster?.context && app.configSection === 'contexts'}>
						<summary>
							<span class="swatch" style:background={settings.color || 'transparent'}></span>
							<strong>{app.contextName(ctx.name)}</strong>
							{#if settings.displayName}<span class="muted small">{ctx.name}</span>{/if}
							{#if settings.production}<span class="tag bad">production</span>
							{:else if settings.label}<span class="tag">{settings.label}</span>{/if}
							{#if settings.readOnly}<span class="tag">read-only</span>
							{:else if settings.locked}<span class="tag">locked</span>{/if}
							{#if ctx.name === app.cluster?.context}<span class="tag ok">connected</span>{/if}
						</summary>
						<div class="context-form">
							<ContextSettingsForm
								context={ctx.name}
								namespaces={ctx.name === app.cluster?.context ? app.namespaces : []}
							/>
						</div>
					</details>
				{/each}
			</div>
		</div>

		<div class="block">
			<h2 id="config-diffs">Dry-run diffs</h2>
			<p class="muted">
				Before these actions run, Kubri asks the API server for a dry run and shows what would change. YAML edits always
				show it.
			</p>
			<div class="diff-settings">
				{#each [['scale', 'Scale'], ['restart', 'Restart'], ['image', 'Set image'], ['labels', 'Labels & annotations']] as const as [key, label] (key)}
					<label>
						<input
							type="checkbox"
							checked={app.settings.diff[key]}
							onchange={(e) => app.setDiffSettings({ ...app.settings.diff, [key]: e.currentTarget.checked })}
						/>
						{label}
					</label>
				{/each}
			</div>
		</div>

		<div class="block">
			<h2 id="config-changes">What changed</h2>
			<p class="muted">
				While it runs, Kubri records what changes in the context it is connected to, in <code>history.db</code> in its config
				folder. ConfigMap and Secret values are never kept, only which keys changed.
			</p>
			<div class="diff-settings">
				<label>
					<input
						type="checkbox"
						checked={app.settings.history.record}
						onchange={(e) => app.setHistorySettings({ ...app.settings.history, record: e.currentTarget.checked })}
					/>
					Record changes
				</label>
				<label>
					Keep for
					<input
						class="number"
						type="number"
						min="1"
						max="365"
						value={app.settings.history.keepDays}
						onchange={(e) => {
							const days = Math.round(Number(e.currentTarget.value));
							if (days >= 1) app.setHistorySettings({ ...app.settings.history, keepDays: Math.min(days, 365) });
						}}
					/>
					days
				</label>
				<label>
					At most
					<input
						class="number"
						type="number"
						min="10"
						max="10000"
						step="10"
						value={app.settings.history.maxMb}
						onchange={(e) => {
							const mb = Math.round(Number(e.currentTarget.value));
							if (mb >= 10) app.setHistorySettings({ ...app.settings.history, maxMb: Math.min(mb, 10000) });
						}}
					/>
					MB
				</label>
			</div>
		</div>

		<div class="block">
			<h2 id="config-appearance">Appearance</h2>
			<p class="muted">Kubri's own colors. A k9s skin, picked per context below, takes their place while it is on.</p>
			<div class="skins themes" role="radiogroup" aria-label="Colors">
				{#each schemes as option (option.label)}
					<button
						class="skin"
						class:active={app.scheme === option.value}
						role="radio"
						aria-checked={app.scheme === option.value}
						onclick={() => app.setScheme(option.value)}
					>
						{@render swatch(option.theme)}
						<span class="name">{option.label}</span>
					</button>
				{/each}
			</div>
			{#if app.skin}<p class="muted small">The skin {app.skin} is on screen, so these show once it is taken off.</p>{/if}
			<div class="zoom">
				<span>Zoom</span>
				{#if desktop}
					<button onclick={() => app.setZoom(stepZoom(app.zoom, -1))} disabled={app.zoom <= ZOOMS[0]} aria-label="Zoom out">−</button>
					<select value={app.zoom} onchange={(e) => app.setZoom(Number(e.currentTarget.value))} aria-label="Zoom level">
						{#each ZOOMS as z (z)}<option value={z}>{Math.round(z * 100)}%</option>{/each}
					</select>
					<button onclick={() => app.setZoom(stepZoom(app.zoom, 1))} disabled={app.zoom >= ZOOMS[ZOOMS.length - 1]} aria-label="Zoom in">+</button>
					<span class="muted small"><kbd>Ctrl +</kbd> <kbd>Ctrl −</kbd> and <kbd>Ctrl 0</kbd> too; kept for the next start</span>
				{:else}
					<span class="muted small">Your browser's zoom (<kbd>Ctrl +</kbd>, <kbd>Ctrl −</kbd>), which it keeps for this page</span>
				{/if}
			</div>
		</div>

		<div class="block">
			<h2 id="config-updates">Updates</h2>
			<div class="diff-settings">
				<label>
					<input type="checkbox" checked={app.releaseChecks} onchange={(e) => app.setReleaseChecks(e.currentTarget.checked)} />
					Tell me when a new release is out
				</label>
			</div>
			<p class="muted small">
				Kubri asks GitHub for its latest release when it starts, at most every six hours, and shows a notice at the bottom
				when it is newer than the one running. Nothing else is sent.
				{#if backend.desktop}
					When Kubri can update itself, it asks whether to: it then downloads the release, checks its signature, installs
					it and starts again. Installs from Flatpak, the AUR or another package manager are updated there.
				{/if}
			</p>
		</div>

		<AgentSettings />

		<ExplainSettings />

		<div class="block files">
			<h2 id="config-views">Views <span class="count">{app.views?.views.length ?? 0}</span></h2>
			<p class="muted">
				How custom resources show: extra columns, a Health column, sections in the overview, actions and edges in the
				graph, each from a YAML file. Kubri brings views for Flux, Argo CD and cert-manager; yours go in
				{#if app.views?.dir}<code>{app.views.dir}</code>{:else}the <code>views</code> folder next to the settings file{/if}
				and replace a built-in one of the same kind. <a href="https://github.com/snaketech-tu/kubri/blob/main/docs/VIEWS.md" target="_blank" rel="noreferrer">How to write one</a>.
			</p>
			{#if app.views?.errors.length}
				<ul>
					{#each app.views.errors as error (error)}
						<li class="bad"><span class="mark" aria-hidden="true">✕</span><span class="error">{error}</span></li>
					{/each}
				</ul>
			{/if}
			{#if app.views?.views.length}
				<table>
					<thead><tr><th>Kind</th><th>Shows</th><th>File</th></tr></thead>
					<tbody>
						{#each app.views.views as view (`${view.group}/${view.kind}`)}
							{@const served = app.resources.some((r) => r.group === view.group && r.kind === view.kind)}
							<tr>
								<td>
									<div>{view.kind}</div>
									<div class="muted small">{view.group || 'core'}{served ? '' : ' · not in this cluster'}</div>
								</td>
								<td class="muted">{shows(view)}</td>
								<td class="mono">{view.source}</td>
							</tr>
						{/each}
					</tbody>
				</table>
			{/if}
		</div>

		{#if !config}
			<p class="empty">Reading the k9s configuration…</p>
		{:else}
			<div class="block files">
				<h2 id="config-files">Files <span class="count">{config.files.length}</span></h2>
				<p>
					Kubri reads the files k9s reads, for the context <strong>{app.cluster?.context ?? '–'}</strong>:
					{#each config.dirs as dir, i (dir)}{i ? ' and ' : ''}<code>{dir}</code>{/each}.
				</p>
				{#if config.files.length === 0}
					<p class="muted">
						No k9s files yet. Put <code>aliases.yaml</code>, <code>hotkeys.yaml</code>, <code>plugins.yaml</code> or a
						<code>skins</code> folder there, and press Reload.
					</p>
				{:else}
					<ul>
						{#each config.files as file (file.path)}
							<li class:bad={file.error}>
								<span class="mark" aria-hidden="true">{file.error ? '✕' : '✓'}</span>
								<code>{file.path}</code>
								{#if file.error}<div class="error">{file.error}</div>{/if}
							</li>
						{/each}
					</ul>
				{/if}
				{#if broken.length}
					<p class="error">{broken.length === 1 ? 'One file' : `${broken.length} files`} could not be read; the rest still works.</p>
				{/if}
			</div>

			<div class="block">
				<h2 id="config-skins">Skin</h2>
				<p class="muted">
					Picked per context, like in k9s. <code>:skin name</code> switches from the command line.
				</p>
				{#if app.skinError}<p class="error">{app.skinError}</p>{/if}
				{#if app.skin && skinWarning(previews[app.skin])}
					<p class="warning">{app.skin}: {skinWarning(previews[app.skin])}. Kubri shows it as it is; another skin or Kubri's colors read better.</p>
				{/if}
				<div class="skins">
					<button class="skin" class:active={app.skinChoice === undefined} onclick={() => app.setSkin(undefined)}>
						{@render swatch(config.skin ? previews[config.skin] : kubriTheme)}
						<span class="name">As in k9s</span>
						<span class="muted small">{config.skin ?? 'no skin set'}</span>
					</button>
					<button class="skin" class:active={app.skinChoice === null} onclick={() => app.setSkin(null)}>
						{@render swatch(kubriTheme)}
						<span class="name">Kubri</span>
						<span class="muted small">light or dark, as the system</span>
					</button>
					{#each config.skins as name (name)}
						{@const warning = skinWarning(previews[name])}
						<button class="skin" class:active={app.skinChoice === name} onclick={() => app.setSkin(name)}>
							{@render swatch(previews[name])}
							<span class="name">{name}</span>
							{#if app.skin === name}<span class="muted small">on screen</span>{/if}
							{#if warning}<span class="warning small" title={warning}>Hard to read</span>{/if}
						</button>
					{/each}
				</div>
				{#if config.skins.length === 0}
					<p class="muted">No skins found. k9s skins go in a <code>skins</code> folder next to the other files.</p>
				{/if}
			</div>

			<div class="block">
				<h2 id="config-hotkeys">Hotkeys <span class="count">{config.hotkeys.length}</span></h2>
				{#if config.hotkeys.length}
					<table>
						<thead><tr><th>Key</th><th>Description</th><th>Command</th><th></th></tr></thead>
						<tbody>
							{#each config.hotkeys as hotkey (hotkey.name)}
								{@const k = key(hotkey.shortCut)}
								<tr>
									<td>{@render keyCell(hotkey.shortCut, k, hotkey.override)}</td>
									<td>{hotkey.description || hotkey.name}</td>
									<td class="mono">:{hotkey.command}</td>
									<td class="actions"><button class="btn small" onclick={() => runHotkey(hotkey)}>Run</button></td>
								</tr>
							{/each}
						</tbody>
					</table>
				{:else}
					<p class="muted">No hotkeys. In <code>hotkeys.yaml</code>, a hotkey opens a view with a key, like <code>Shift-0</code> for <code>:pods</code>.</p>
				{/if}
			</div>

			<div class="block">
				<h2 id="config-plugins">Plugins <span class="count">{config.plugins.length}</span></h2>
				{#if config.plugins.length}
					<table>
						<thead><tr><th>Key</th><th>Plugin</th><th>Where</th><th>Command</th><th></th></tr></thead>
						<tbody>
							{#each config.plugins as plugin (plugin.name)}
								{@const k = key(plugin.shortCut)}
								<tr>
									<td>{@render keyCell(plugin.shortCut, k, plugin.override)}</td>
									<td>
										<div>{plugin.name}</div>
										{#if plugin.description}<div class="muted small">{plugin.description}</div>{/if}
									</td>
									<td class="muted">{plugin.scopes.join(', ') || 'nowhere'}</td>
									<td class="mono command" title={[plugin.command, ...plugin.args].join(' ')}>
										{[plugin.command, ...plugin.args].join(' ')}
									</td>
									<td class="tags">
										{#if plugin.background}<span class="tag">background</span>{/if}
										{#if plugin.confirm}<span class="tag">asks first</span>{/if}
										{#if plugin.dangerous}<span class="tag bad">dangerous</span>{/if}
									</td>
								</tr>
							{/each}
						</tbody>
					</table>
					<p class="muted small">
						Plugins run on this computer, in the views their scopes name. Terminal plugins open over the window; the
						variables like <code>$NAME</code> and <code>$NAMESPACE</code> work as in k9s.
					</p>
				{:else}
					<p class="muted">No plugins. In <code>plugins.yaml</code>, a plugin runs a command for the selected object.</p>
				{/if}
			</div>

			<div class="block">
				<h2 id="config-aliases">Aliases <span class="count">{config.aliases.length}</span></h2>
				{#if config.aliases.length}
					<div class="aliases">
						{#each config.aliases as alias (alias.name)}
							<button class="alias" onclick={() => runCommand(alias.name)} title="Open :{alias.name}">
								<code class="name">{alias.name}</code>
								<span class="muted">→</span>
								<code>{alias.command}</code>
							</button>
						{/each}
					</div>
				{:else}
					<p class="muted">No aliases. In <code>aliases.yaml</code>, <code>pp: v1/pods</code> makes <code>:pp</code> open pods.</p>
				{/if}
			</div>
		{/if}
	</div>
</section>

{#snippet swatch(theme: SkinTheme | null | undefined)}
	{@const v = theme?.vars ?? {}}
	<span class="swatch" style:background={v['--bg'] ?? (theme?.scheme === 'light' ? '#f6f7fb' : '#101522')}>
		<span class="line" style:background={v['--text'] ?? (theme?.scheme === 'light' ? '#20283d' : '#e5eaf5')}></span>
		<span class="line short" style:background={v['--accent'] ?? '#a6b3ff'}></span>
		<span class="dots">
			<span style:background={v['--ok'] ?? '#3ccf7e'}></span>
			<span style:background={v['--warn'] ?? '#e5a73b'}></span>
			<span style:background={v['--bad'] ?? '#f06b6b'}></span>
		</span>
	</span>
{/snippet}

{#snippet keyCell(text: string, k: { label: string; taken: boolean } | null, override: boolean)}
	{#if !k}
		<span class="error" title="Kubri binds letters, digits and F1 to F12, with Ctrl, Shift or Alt">{text || 'none'}?</span>
	{:else}
		<kbd>{k.label}</kbd>
		{#if override}
			<span class="muted small" title="Takes the key even where Kubri uses it">override</span>
		{:else if k.taken}
			<span class="warn small" title="Kubri uses this key in some views; there it stays Kubri's unless override: true">
				shared
			</span>
		{/if}
	{/if}
{/snippet}

<style>
	section {
		display: flex;
		flex-direction: column;
		min-height: 0;
		min-width: 0;
		padding: 0 24px;
	}
	.scroll {
		flex: 1;
		overflow: auto;
		padding-bottom: 20px;
	}
	.block {
		margin-bottom: 14px;
		padding: 16px 18px;
		border: 1px solid var(--border);
		border-radius: 10px;
		background: var(--surface-1);
	}
	h2 {
		display: flex;
		align-items: center;
		gap: 8px;
		margin: 0 0 8px;
		font-size: 15px;
	}
	p {
		margin: 6px 0;
	}
	.files ul {
		margin: 8px 0 0;
		padding: 0;
		list-style: none;
	}
	.files li {
		padding: 3px 0;
	}
	.mark {
		display: inline-block;
		width: 16px;
		color: var(--ok);
		font-weight: 700;
	}
	li.bad .mark {
		color: var(--bad);
	}
	li .error {
		margin-left: 16px;
		font-size: 12px;
		white-space: pre-wrap;
	}
	code,
	.mono {
		font-family: var(--mono);
		font-size: 12.5px;
	}
	.skins {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
		gap: 10px;
		margin-top: 10px;
	}
	.skin {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 4px;
		padding: 8px;
		border: 1px solid var(--border);
		border-radius: 10px;
		background: var(--surface-1);
		text-align: left;
	}
	.skin:hover {
		border-color: var(--accent);
	}
	.skin.active {
		border-color: var(--accent);
		box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent) 35%, transparent);
	}
	.skin .name {
		font-weight: 600;
		overflow-wrap: anywhere;
	}
	.themes {
		max-width: 520px;
	}
	.warning {
		color: var(--warn);
	}
	.zoom {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 8px;
		margin-top: 14px;
	}
	.zoom > span:first-child {
		font-weight: 600;
		margin-right: 4px;
	}
	.zoom button,
	.zoom select {
		min-width: 32px;
		padding: 4px 8px;
		border: 1px solid var(--border);
		border-radius: 6px;
		background: var(--surface-1);
		color: var(--text);
	}
	.swatch {
		display: flex;
		flex-direction: column;
		gap: 5px;
		width: 100%;
		height: 58px;
		padding: 9px 10px;
		border: 1px solid var(--border-subtle);
		border-radius: 6px;
	}
	.line {
		height: 5px;
		width: 80%;
		border-radius: 3px;
	}
	.line.short {
		width: 45%;
	}
	.dots {
		display: flex;
		gap: 4px;
		margin-top: auto;
	}
	.dots span {
		width: 9px;
		height: 9px;
		border-radius: 50%;
	}
	table {
		width: 100%;
		border-collapse: collapse;
		font-size: 13px;
	}
	th {
		padding: 8px 10px;
		border-bottom: 1px solid var(--border);
		text-align: left;
		font-size: 12px;
		font-weight: 600;
		color: var(--text-muted);
	}
	td {
		padding: 6px 10px;
		border-bottom: 1px solid var(--border-subtle);
		vertical-align: top;
	}
	td:first-child {
		white-space: nowrap;
	}
	.command {
		max-width: 360px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.actions {
		text-align: right;
	}
	.tags {
		text-align: right;
		white-space: nowrap;
	}
	.tag {
		margin-left: 4px;
		padding: 1px 7px;
		border-radius: 999px;
		background: var(--surface-2);
		font-size: 11px;
	}
	.tag.bad {
		background: color-mix(in srgb, var(--bad) 16%, transparent);
		color: var(--bad);
	}
	.tag.ok {
		background: color-mix(in srgb, var(--ok) 16%, transparent);
		color: var(--ok);
	}
	.diff-settings {
		display: flex;
		flex-wrap: wrap;
		gap: 8px 20px;
	}
	.diff-settings label {
		display: inline-flex;
		align-items: center;
		gap: 6px;
	}
	.number {
		width: 72px;
	}
	.contexts {
		display: grid;
		gap: 6px;
	}
	.context {
		border: 1px solid var(--border-subtle);
		border-radius: 8px;
	}
	.context summary {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 8px 12px;
		cursor: pointer;
	}
	.context[open] summary {
		border-bottom: 1px solid var(--border-subtle);
	}
	.context .swatch {
		width: 10px;
		height: 10px;
		flex: none;
		border-radius: 50%;
		box-shadow: 0 0 0 1px var(--border);
	}
	.context-form {
		max-width: 420px;
		padding: 12px;
	}
	.aliases {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
		gap: 4px 12px;
	}
	.alias {
		display: flex;
		align-items: baseline;
		gap: 8px;
		min-width: 0;
		padding: 4px 8px;
		border: none;
		border-radius: 6px;
		background: none;
		text-align: left;
	}
	.alias:hover {
		background: var(--surface-2);
	}
	.alias code {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.alias .name {
		flex-shrink: 0;
		font-weight: 600;
		color: var(--accent-strong);
	}
	.count {
		padding: 1px 8px;
		border-radius: 999px;
		background: var(--surface-2);
		font-size: 12px;
		font-weight: 600;
	}
	kbd {
		margin-right: 6px;
		padding: 0 5px;
		border: 1px solid var(--border);
		border-radius: 4px;
		background: var(--surface-2);
		font-family: var(--mono);
		font-size: 11.5px;
	}
	.muted {
		color: var(--text-muted);
	}
	.small {
		font-size: 11.5px;
	}
	.warn {
		color: var(--warn);
	}
	.error {
		color: var(--bad);
	}
	.empty {
		padding: 40px 0;
		text-align: center;
		color: var(--text-muted);
	}
	@media (max-width: 720px) {
		section {
			padding: 0 12px;
		}
		.block {
			padding: 14px 12px;
		}
	}
</style>
