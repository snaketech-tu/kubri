<script lang="ts">

	import { tick } from 'svelte';
	import { runCommand, runHotkey } from '../actions';
	import { isTauri } from '@tauri-apps/api/core';
	import { backend, type KindView } from '../api';
	import { ZOOMS, skinWarning, stepZoom } from '../appearance';
	import { setKubectlUnits, units } from '../units.svelte';
	import { kubriUses, parseShortcut, shortcutLabel, skinTheme, type SkinTheme } from '../k9s';
	import { forControl } from '../keys';
	import { resolveColor, type Scheme } from '../skin';
	import { app } from '../state.svelte';
	import { LANGUAGES, languageSetting, parts, plural, setLanguage, t, type LanguageSetting } from '../i18n';
	import AgentSettings from './AgentSettings.svelte';
	import ExplainSettings from './ExplainSettings.svelte';
	import ContextSettingsForm from './ContextSettingsForm.svelte';
	import PageHeader from './PageHeader.svelte';
	import Phrase from './Phrase.svelte';
	import Select from './Select.svelte';
	import { setStartSetting, startSetting, type StartSetting } from '../startup';

	let previews = $state<Record<string, SkinTheme | null>>({});
	let start = $state<StartSetting>(startSetting());
	let reloading = $state(false);
	let scroller = $state<HTMLDivElement>();

	const config = $derived(app.k9s);
	/** The sections of the page, in order; the k9s ones only once the files are read. */
	const sectionList = $derived<[string, string][]>([
		['contexts', t('Contexts')],
		['diffs', t('Dry-run diffs')],
		['changes', t('What changed')],
		['start', t('On start')],
		['recent', t('Recent contexts')],
		...(app.localTerminal ? ([['terminal', t('Terminal')]] as [string, string][]) : []),
		['appearance', t('Appearance')],
		['updates', t('Updates')],
		['agents', t('AI agents')],
		['explain', t('Explain')],
		['views', t('Views')],
		...(config
			? ([['files', t('k9s files')], ['skins', t('Skin')], ['hotkeys', t('Hotkeys')], ['plugins', t('Plugins')], ['aliases', t('Aliases')]] as [string, string][])
			: [])
	]);
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

	/** The section in view, marked in the list on the left: the last one whose heading is near the top. */
	let current = $state('contexts');
	function spy() {
		if (!scroller) return;
		const top = scroller.getBoundingClientRect().top + 48;
		let found = sectionList[0]?.[0] ?? 'contexts';
		for (const [id] of sectionList) {
			const heading = scroller.querySelector(`#config-${id}`);
			if (heading && heading.getBoundingClientRect().top <= top) found = id;
		}
		// At the very bottom, the last sections cannot reach the top; the last one is the one in view.
		if (scroller.scrollTop + scroller.clientHeight >= scroller.scrollHeight - 2) found = sectionList.at(-1)?.[0] ?? found;
		current = found;
	}
	$effect(() => {
		void sectionList;
		tick().then(spy);
	});

	async function reload() {
		reloading = true;
		requested.clear();
		previews = {};
		await Promise.all([app.loadK9s(), app.views.load()]);
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
		vars: { '--bg': '#f6f7fb', '--text': '#20283d', '--accent': '#4b64f4', '--ok': '#157a42', '--warn': '#9a5f00', '--bad': '#c93a3a' },
		scheme: 'light'
	};
	const darkTheme: SkinTheme = {
		vars: { '--bg': '#101522', '--text': '#e5eaf5', '--accent': '#a6b3ff', '--ok': '#3ccf7e', '--warn': '#e5a73b', '--bad': '#f06b6b' },
		scheme: 'dark'
	};
	const kubriTheme = $derived(app.appearance.systemDark ? darkTheme : lightTheme);
	const desktop = isTauri();
	const schemes: { value: Scheme; label: string; theme: SkinTheme }[] = $derived([
		{ value: null, label: t('As the system'), theme: kubriTheme },
		{ value: 'light', label: t('Light'), theme: lightTheme },
		{ value: 'dark', label: t('Dark'), theme: darkTheme }
	]);
</script>

<svelte:window {onkeydown} />

<section>
	<PageHeader
		crumbs={[t('Settings')]} prefix="none"
		title={t('Settings')}
		subtitle={t("Kubri's own settings, and the aliases, hotkeys, plugins and skins from your k9s files")}
	>
		{#snippet tools()}
			<button class="btn" onclick={reload} disabled={reloading} title={t('Read the files again (Shift+R)')}>
				{reloading ? t('Reading…') : t('Reload k9s files')}
			</button>
		{/snippet}
	</PageHeader>

	<div class="layout">
	<nav class="sections" aria-label={t('Sections')}>
		{#each sectionList as [id, label] (id)}
			<button
				aria-current={current === id ? 'location' : undefined}
				onclick={() => scroller?.querySelector(`#config-${id}`)?.scrollIntoView({ block: 'start', behavior: 'smooth' })}
			>
				{label}
			</button>
		{/each}
	</nav>
	<div class="scroll" bind:this={scroller} onscroll={spy}>
		<div class="block">
			<h2 id="config-contexts">{t('Contexts')} <span class="count">{app.contexts.length}</span></h2>
			<p class="muted">
				{t('Names, colors and guard rails per context, kept in Kubri\'s settings file. The kubeconfig is never changed.')}
			</p>
			<div class="contexts">
				{#each app.contexts as ctx (ctx.name)}
					{@const settings = app.prefs.contextSettingsOf(ctx.name)}
					<details class="context" open={ctx.name === app.cluster?.context && app.configSection === 'contexts'}>
						<summary>
							{#if settings.color}
								<span class="swatch" style:background={settings.color} title={t('Color {color}', { color: settings.color })} aria-hidden="true"></span>
							{/if}
							<strong>{app.prefs.contextName(ctx.name)}</strong>
							{#if settings.displayName}<span class="muted small">{ctx.name}</span>{/if}
							{#if settings.production}<span class="tag bad">{t('production')}</span>
							{:else if settings.label}<span class="tag">{settings.label}</span>{/if}
							{#if settings.readOnly}<span class="tag">{t('read-only')}</span>
							{:else if settings.locked}<span class="tag">{t('locked')}</span>{/if}
							{#if ctx.name === app.cluster?.context}<span class="tag ok">{t('connected')}</span>{/if}
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
			<h2 id="config-diffs">{t('Dry-run diffs')}</h2>
			<p class="muted">
				{t('Before these actions run, Kubri asks the API server for a dry run and shows what would change. YAML edits always show it.')}
			</p>
			<div class="diff-settings">
				{#each [['scale', t('Scale')], ['restart', t('Restart')], ['image', t('Set image')], ['labels', t('Labels & annotations')]] as const as [key, label] (key)}
					<label>
						<input
							type="checkbox"
							checked={app.prefs.settings.diff[key]}
							onchange={(e) => app.prefs.setDiffSettings({ ...app.prefs.settings.diff, [key]: e.currentTarget.checked })}
						/>
						{label}
					</label>
				{/each}
			</div>
		</div>

		<div class="block">
			<h2 id="config-changes">{t('What changed')}</h2>
			<p class="muted">
				<Phrase
					text={t('While it runs, Kubri records what changes in the context it is connected to, in {file} in its config folder. ConfigMap and Secret values are never kept, only which keys changed.')}
					code={{ file: 'history.db' }}
				/>
			</p>
			<div class="diff-settings">
				<label>
					<input
						type="checkbox"
						checked={app.prefs.settings.history.record}
						onchange={(e) => app.prefs.setHistorySettings({ ...app.prefs.settings.history, record: e.currentTarget.checked })}
					/>
					{t('Record changes')}
				</label>
				<label>
					{t('Keep for')}
					<input
						class="number"
						type="number"
						min="1"
						max="365"
						value={app.prefs.settings.history.keepDays}
						onchange={(e) => {
							const days = Math.round(Number(e.currentTarget.value));
							if (days >= 1) app.prefs.setHistorySettings({ ...app.prefs.settings.history, keepDays: Math.min(days, 365) });
						}}
					/>
					{t('days')}
				</label>
				<label>
					{t('At most')}
					<input
						class="number"
						type="number"
						min="10"
						max="10000"
						step="10"
						value={app.prefs.settings.history.maxMb}
						onchange={(e) => {
							const mb = Math.round(Number(e.currentTarget.value));
							if (mb >= 10) app.prefs.setHistorySettings({ ...app.prefs.settings.history, maxMb: Math.min(mb, 10000) });
						}}
					/>
					MB
				</label>
			</div>
		</div>

		<div class="block">
			<h2 id="config-start">{t('On start')}</h2>
			<p class="muted">
				{t('Kubri opens the context you used last, where you left it. When your kubeconfig names another current context since then, Kubri follows the kubeconfig.')}
			</p>
			<div class="diff-settings">
				<label>
					{t('Connect to')}
					<Select
						value={start}
						options={[
							{ value: 'last' as StartSetting, label: t('The context used last') },
							{ value: 'kubeconfig' as StartSetting, label: t("The kubeconfig's current context") }
						]}
						onchange={(value) => {
							start = value;
							setStartSetting(value);
						}}
						aria-label={t('Context on start')}
						compact
					/>
				</label>
			</div>
		</div>

		<div class="block">
			<h2 id="config-recent">{t('Recent contexts')}</h2>
			<p class="muted">
				{t('Contexts you switch away from stay connected for up to 30 minutes, so switching back is instant and lands where you left. They are marked with a green dot in the context switcher.')}
			</p>
			<div class="diff-settings">
				<label>
					{t('Keep')}
					<input
						class="number"
						type="number"
						min="0"
						max="5"
						value={app.prefs.settings.connections?.keepRecent ?? 3}
						onchange={(e) => {
							const count = Math.round(Number(e.currentTarget.value));
							if (count >= 0) app.prefs.setConnectionSettings({ keepRecent: Math.min(count, 5) });
						}}
					/>
					{t('contexts connected (0 turns it off)')}
				</label>
			</div>
		</div>

		{#if app.localTerminal}
			{@const terminal = app.prefs.settings.terminal ?? { shell: '', args: [], startDir: '', fontSize: 13, fontFamily: '' }}
			<div class="block">
				<h2 id="config-terminal">{t('Terminal')}</h2>
				<p class="muted">
					<Phrase
						text={backend.desktop
							? t('{command} or {keys} opens a shell on this computer in which kubectl and helm use the context and namespace Kubri shows. Leave the shell empty for your login shell, or PowerShell on Windows.')
							: t("{command} or {keys} opens a shell on the server in which kubectl and helm use the context and namespace Kubri shows. Leave the shell empty for the server's login shell.")}
						code={{ command: ':term' }}
						kbd={{ keys: 'Ctrl+`' }}
					/>
				</p>
				<div class="diff-settings">
					<label>
						{t('Shell')}
						<input
							class="path"
							type="text"
							placeholder={t('Login shell')}
							spellcheck="false"
							value={terminal.shell}
							onchange={(e) => app.prefs.setTerminalSettings({ ...terminal, shell: e.currentTarget.value.trim() })}
						/>
					</label>
					<label>
						{t('Arguments')}
						<input
							class="path"
							type="text"
							placeholder={t('None')}
							spellcheck="false"
							title={t('Separated by spaces')}
							value={terminal.args.join(' ')}
							onchange={(e) => app.prefs.setTerminalSettings({ ...terminal, args: e.currentTarget.value.split(/\s+/).filter(Boolean) })}
						/>
					</label>
					<label>
						{t('Start in')}
						<input
							class="path"
							type="text"
							placeholder={t('Home folder')}
							spellcheck="false"
							value={terminal.startDir}
							onchange={(e) => app.prefs.setTerminalSettings({ ...terminal, startDir: e.currentTarget.value.trim() })}
						/>
					</label>
					<label>
						{t('Font size')}
						<input
							class="number"
							type="number"
							min="8"
							max="32"
							value={terminal.fontSize}
							onchange={(e) => {
								const size = Math.round(Number(e.currentTarget.value));
								if (size >= 8 && size <= 32) app.prefs.setTerminalSettings({ ...terminal, fontSize: size });
							}}
						/>
						{t('px, for terminals opened next')}
					</label>
					<label>
						{t('Font')}
						<input
							class="path"
							type="text"
							placeholder={t('Kubri\'s font')}
							spellcheck="false"
							title={t('Installed fonts to use first, separated by commas, like MesloLGS NF. Kubri\'s own font and its icons for prompts like powerlevel10k fill in the rest.')}
							value={terminal.fontFamily}
							onchange={(e) => app.prefs.setTerminalSettings({ ...terminal, fontFamily: e.currentTarget.value.trim() })}
						/>
					</label>
				</div>
			</div>
		{/if}

		<div class="block">
			<h2 id="config-appearance">{t('Appearance')}</h2>
			<p class="muted">{t('Kubri\'s own colors. A k9s skin, picked per context below, takes their place while it is on.')}</p>
			<div class="skins themes" role="radiogroup" aria-label={t('Colors')}>
				{#each schemes as option (option.label)}
					<button
						class="skin"
						class:active={app.appearance.scheme === option.value}
						role="radio"
						aria-checked={app.appearance.scheme === option.value}
						onclick={() => {
							app.appearance.setScheme(option.value);
							if (app.appearance.skin) app.appearance.setSkin(null);
						}}
					>
						{@render swatch(option.theme)}
						<span class="name">{option.label}</span>
					</button>
				{/each}
			</div>
			{#if app.appearance.skin}<p class="muted small">{t('The skin {skin} is on screen. Picking one of these takes it off for this context.', { skin: app.appearance.skin })}</p>{/if}
			<div class="zoom">
				<span>{t('Zoom')}</span>
				{#if desktop}
					<button onclick={() => app.appearance.setZoom(stepZoom(app.appearance.zoom, -1))} disabled={app.appearance.zoom <= ZOOMS[0]} aria-label={t('Zoom out')}>−</button>
					<Select
						value={app.appearance.zoom}
						options={ZOOMS.map((z) => ({ value: z, label: `${Math.round(z * 100)}%` }))}
						onchange={(z) => app.appearance.setZoom(z)}
						aria-label={t('Zoom level')}
						compact
					/>
					<button onclick={() => app.appearance.setZoom(stepZoom(app.appearance.zoom, 1))} disabled={app.appearance.zoom >= ZOOMS[ZOOMS.length - 1]} aria-label={t('Zoom in')}>+</button>
					<span class="muted small"
						><Phrase text={t('{bigger} {smaller} and {reset} too; kept for the next start')} kbd={{ bigger: 'Ctrl +', smaller: 'Ctrl −', reset: 'Ctrl 0' }} /></span
					>
				{:else}
					<span class="muted small"
						><Phrase text={t("Your browser's zoom ({bigger}, {smaller}), which it keeps for this page")} kbd={{ bigger: 'Ctrl +', smaller: 'Ctrl −' }} /></span
					>
				{/if}
			</div>
			<div class="zoom">
				<span>{t('Language')}</span>
				<Select
					value={languageSetting()}
					options={[{ value: 'system' as LanguageSetting, label: t('As the system') }, ...LANGUAGES]}
					onchange={(value) => setLanguage(value)}
					aria-label={t('Language')}
					compact
				/>
				<span class="muted small">{t('Kubri starts again in the language you pick.')}</span>
			</div>
			<div class="zoom">
				<span>{t('CPU and memory')}</span>
				<Select
					value={units.kubectl ? 'kubectl' : 'plain'}
					options={[
						{ value: 'plain', label: t('In cores and MiB (0.25, 512 MiB)') },
						{ value: 'kubectl', label: t('Like kubectl (250m, 512Mi)') }
					]}
					onchange={(value) => setKubectlUnits(value === 'kubectl')}
					aria-label={t('CPU and memory')}
					compact
				/>
				<span class="muted small">{t('How lists and the overview show amounts. The CSV export and k9s plugins always get them like kubectl.')}</span>
			</div>
		</div>

		<div class="block">
			<h2 id="config-updates">{t('Updates')}</h2>
			<div class="diff-settings">
				<label>
					<input type="checkbox" checked={app.releases.checks} onchange={(e) => app.releases.setChecks(e.currentTarget.checked)} />
					{t('Tell me when a new release is out')}
				</label>
			</div>
			<p class="muted small">
				{t('Kubri asks GitHub for its latest release when it starts, at most every six hours, and shows a notice at the bottom when it is newer than the one running. Nothing else is sent.')}
				{#if backend.desktop}
					{t('When Kubri can update itself, it asks whether to: it then downloads the release, checks its signature, installs it and starts again. Installs from Flatpak, the AUR or another package manager are updated there.')}
				{/if}
			</p>
		</div>

		<AgentSettings />

		<ExplainSettings />

		<div class="block files">
			<h2 id="config-views">{t('Views')} <span class="count">{app.views.loaded?.views.length ?? 0}</span></h2>
			<p class="muted">
				{#each parts(t('How custom resources show: extra columns, a Health column, sections in the overview, actions and edges in the graph, each from a YAML file. Kubri brings views for Flux, Argo CD and cert-manager; yours go in {folder} and replace a built-in one of the same kind. {link}.')) as part, i (i)}{#if 'text' in part}{part.text}{:else if part.name === 'folder'}{#if app.views.loaded?.dir}<code
								>{app.views.loaded.dir}</code
							>{:else}<Phrase text={t('the {views} folder next to the settings file')} code={{ views: 'views' }} />{/if}{:else}<a
							href="https://github.com/snaketech-tu/kubri/blob/main/docs/VIEWS.md"
							target="_blank"
							rel="noreferrer">{t('How to write one')}</a
						>{/if}{/each}
			</p>
			{#if app.views.loaded?.errors.length}
				<ul>
					{#each app.views.loaded.errors as error (error)}
						<li class="bad"><span class="mark" aria-hidden="true">✕</span><span class="error">{error}</span></li>
					{/each}
				</ul>
			{/if}
			{#if app.views.loaded?.views.length}
				<table>
					<thead><tr><th>{t('Kind')}</th><th>{t('Shows')}</th><th>{t('File')}</th></tr></thead>
					<tbody>
						{#each app.views.loaded.views as view (`${view.group}/${view.kind}`)}
							{@const served = app.resources.some((r) => r.group === view.group && r.kind === view.kind)}
							<tr>
								<td>
									<div>{view.kind}</div>
									<div class="muted small">{view.group || 'core'}{served ? '' : t(' · not in this cluster')}</div>
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
			<p class="empty">{t('Reading the k9s configuration…')}</p>
		{:else}
			<div class="block files">
				<h2 id="config-files">{t('Files')} <span class="count">{config.files.length}</span></h2>
				<p>
					{#each parts(t('Kubri reads the files k9s reads, for the context {context}: {folders}.')) as part, i (i)}{#if 'text' in part}{part.text}{:else if part.name === 'context'}<strong
								>{app.cluster?.context ?? '–'}</strong
							>{:else}{#each config.dirs as dir, j (dir)}{j ? t(' and ') : ''}<code>{dir}</code>{/each}{/if}{/each}
				</p>
				{#if config.files.length === 0}
					<p class="muted">
						<Phrase
							text={t('No k9s files yet. Put {aliases}, {hotkeys}, {plugins} or a {skins} folder there, and press Reload k9s files.')}
							code={{ aliases: 'aliases.yaml', hotkeys: 'hotkeys.yaml', plugins: 'plugins.yaml', skins: 'skins' }}
						/>
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
					<p class="error">{plural(broken.length, 'One file could not be read; the rest still works.', '{n} files could not be read; the rest still works.')}</p>
				{/if}
			</div>

			<div class="block">
				<h2 id="config-skins">{t('Skin')}</h2>
				<p class="muted">
					<Phrase text={t('Picked per context, like in k9s. {command} switches from the command line.')} code={{ command: ':skin name' }} />
				</p>
				{#if app.appearance.skinError}<p class="error">{app.appearance.skinError}</p>{/if}
				{#if app.appearance.skin && skinWarning(previews[app.appearance.skin])}
					<p class="warning">
						{t("{skin}: {warning}. Kubri shows it as it is; another skin or Kubri's colors read better.", { skin: app.appearance.skin, warning: skinWarning(previews[app.appearance.skin]) ?? '' })}
					</p>
				{/if}
				<div class="skins">
					<button class="skin" class:active={app.appearance.skinChoice === undefined} onclick={() => app.appearance.setSkin(undefined)}>
						{@render swatch(config.skin ? previews[config.skin] : kubriTheme)}
						<span class="name">{t('As in k9s')}</span>
						<span class="muted small">{config.skin ?? t('no skin set')}</span>
					</button>
					<button class="skin" class:active={app.appearance.skinChoice === null} onclick={() => app.appearance.setSkin(null)}>
						{@render swatch(kubriTheme)}
						<span class="name">{t('Kubri')}</span>
						<span class="muted small">{t('light or dark, as the system')}</span>
					</button>
					{#each config.skins as name (name)}
						{@const warning = skinWarning(previews[name])}
						<button class="skin" class:active={app.appearance.skinChoice === name} onclick={() => app.appearance.setSkin(name)}>
							{@render swatch(previews[name])}
							<span class="name">{name}</span>
							{#if app.appearance.skin === name}<span class="muted small">{t('on screen')}</span>{/if}
							{#if warning}<span class="warning small" title={warning}>{t('Hard to read')}</span>{/if}
						</button>
					{/each}
				</div>
				{#if config.skins.length === 0}
					<p class="muted"><Phrase text={t('No skins found. k9s skins go in a {skins} folder next to the other files.')} code={{ skins: 'skins' }} /></p>
				{/if}
			</div>

			<div class="block">
				<h2 id="config-hotkeys">{t('Hotkeys')} <span class="count">{config.hotkeys.length}</span></h2>
				{#if config.hotkeys.length}
					<table>
						<thead><tr><th>{t('Key')}</th><th>{t('Description')}</th><th>{t('Command')}</th><th></th></tr></thead>
						<tbody>
							{#each config.hotkeys as hotkey (hotkey.name)}
								{@const k = key(hotkey.shortCut)}
								<tr>
									<td>{@render keyCell(hotkey.shortCut, k, hotkey.override)}</td>
									<td>{hotkey.description || hotkey.name}</td>
									<td class="mono">:{hotkey.command}</td>
									<td class="actions"><button class="btn small" onclick={() => runHotkey(hotkey)}>{t('Run')}</button></td>
								</tr>
							{/each}
						</tbody>
					</table>
				{:else}
					<p class="muted">
						<Phrase
							text={t('No hotkeys. In {file}, a hotkey opens a view with a key, like {key} for {command}.')}
							code={{ file: 'hotkeys.yaml', key: 'Shift-0', command: ':pods' }}
						/>
					</p>
				{/if}
			</div>

			<div class="block">
				<h2 id="config-plugins">{t('Plugins')} <span class="count">{config.plugins.length}</span></h2>
				{#if config.plugins.length}
					<table>
						<thead><tr><th>{t('Key')}</th><th>{t('Plugin')}</th><th>{t('Where')}</th><th>{t('Command')}</th><th></th></tr></thead>
						<tbody>
							{#each config.plugins as plugin (plugin.name)}
								{@const k = key(plugin.shortCut)}
								<tr>
									<td>{@render keyCell(plugin.shortCut, k, plugin.override)}</td>
									<td>
										<div>{plugin.name}</div>
										{#if plugin.description}<div class="muted small">{plugin.description}</div>{/if}
									</td>
									<td class="muted">{plugin.scopes.join(', ') || t('nowhere')}</td>
									<td class="mono command" title={[plugin.command, ...plugin.args].join(' ')}>
										{[plugin.command, ...plugin.args].join(' ')}
									</td>
									<td class="tags">
										{#if plugin.background}<span class="tag">{t('background')}</span>{/if}
										{#if plugin.confirm}<span class="tag">{t('asks first')}</span>{/if}
										{#if plugin.dangerous}<span class="tag bad">{t('dangerous')}</span>{/if}
									</td>
								</tr>
							{/each}
						</tbody>
					</table>
					<p class="muted small">
						<Phrase
							text={t('Plugins run on this computer, in the views their scopes name. Terminal plugins open over the window; the variables like {name} and {namespace} work as in k9s.')}
							code={{ name: '$NAME', namespace: '$NAMESPACE' }}
						/>
					</p>
				{:else}
					<p class="muted"><Phrase text={t('No plugins. In {file}, a plugin runs a command for the selected object.')} code={{ file: 'plugins.yaml' }} /></p>
				{/if}
			</div>

			<div class="block">
				<h2 id="config-aliases">{t('Aliases')} <span class="count">{config.aliases.length}</span></h2>
				{#if config.aliases.length}
					<div class="aliases">
						{#each config.aliases as alias (alias.name)}
							<button class="alias" onclick={() => runCommand(alias.name)} title={t('Open :{name}', { name: alias.name })}>
								<code class="name">{alias.name}</code>
								<span class="muted">→</span>
								<code>{alias.command}</code>
							</button>
						{/each}
					</div>
				{:else}
					<p class="muted">
						<Phrase text={t('No aliases. In {file}, {alias} makes {command} open pods.')} code={{ file: 'aliases.yaml', alias: 'pp: v1/pods', command: ':pp' }} />
					</p>
				{/if}
			</div>
		{/if}
	</div>
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
		<span class="error" title={t('Kubri binds letters, digits and F1 to F12, with Ctrl, Shift or Alt')}>{text || 'none'}?</span>
	{:else}
		<kbd>{k.label}</kbd>
		{#if override}
			<span class="muted small" title={t('Takes the key even where Kubri uses it')}>{t('override')}</span>
		{:else if k.taken}
			<span class="warn small" title={t('Kubri uses this key in some views; there it stays Kubri\'s unless override: true')}>
				{t('shared')}
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
	.layout {
		display: grid;
		grid-template-columns: 170px minmax(0, 1fr);
		gap: 20px;
		flex: 1;
		min-height: 0;
	}
	.sections {
		display: flex;
		flex-direction: column;
		gap: 2px;
		align-self: start;
		position: sticky;
		top: 0;
	}
	.sections button {
		padding: 6px 10px;
		border: none;
		border-radius: 6px;
		background: none;
		color: var(--text-muted);
		font-size: 13px;
		text-align: left;
	}
	.sections button:hover {
		background: var(--surface-2);
		color: var(--text);
	}
	.sections button[aria-current] {
		background: color-mix(in srgb, var(--accent) 12%, transparent);
		color: var(--accent-strong);
		font-weight: 600;
	}
	@media (max-width: 900px) {
		.layout {
			grid-template-columns: minmax(0, 1fr);
		}
		.sections {
			display: none;
		}
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
	.zoom button {
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
	.path {
		width: 24ch;
		font-family: var(--mono);
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
