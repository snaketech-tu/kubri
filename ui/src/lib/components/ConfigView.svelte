<script lang="ts">
	import { tick } from 'svelte';
	import { runCommand, runHotkey } from '../actions';
	import { backend } from '../api';
	import { kubriUses, parseShortcut, shortcutLabel, skinTheme, type SkinTheme } from '../k9s';
	import { forControl } from '../keys';
	import { resolveColor } from '../skin';
	import { app } from '../state.svelte';

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
		await app.loadK9s();
		reloading = false;
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

	// Kubri's own colors, light or dark as the system is.
	const kubriTheme: SkinTheme = matchMedia('(prefers-color-scheme: dark)').matches
		? {
				vars: { '--bg': '#0f131a', '--text': '#e4e8ef', '--accent': '#5b8def', '--ok': '#3ccf7e', '--warn': '#e5a73b', '--bad': '#f06b6b' },
				scheme: 'dark'
			}
		: {
				vars: { '--bg': '#f7f8fa', '--text': '#1b2230', '--accent': '#326ce5', '--ok': '#1f9d55', '--warn': '#c27c0e', '--bad': '#d64545' },
				scheme: 'light'
			};
</script>

<svelte:window {onkeydown} />

<section>
	<header>
		<div class="title">
			<h1>k9s configuration</h1>
			<span class="muted">Aliases, hotkeys, plugins and skins from your k9s files</span>
		</div>
		<button class="reload" onclick={reload} disabled={reloading} title="Read the files again (Shift+R)">
			{reloading ? 'Reading…' : 'Reload'}
		</button>
	</header>

	<div class="scroll" bind:this={scroller}>
		{#if !config}
			<p class="empty">Reading the k9s configuration…</p>
		{:else}
			<div class="files">
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

			<h2 id="config-skins">Skin</h2>
			<p class="muted">
				Picked per context, like in k9s. <code>:skin name</code> switches from the command line.
			</p>
			{#if app.skinError}<p class="error">{app.skinError}</p>{/if}
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
					<button class="skin" class:active={app.skinChoice === name} onclick={() => app.setSkin(name)}>
						{@render swatch(previews[name])}
						<span class="name">{name}</span>
						{#if app.skin === name}<span class="muted small">on screen</span>{/if}
					</button>
				{/each}
			</div>
			{#if config.skins.length === 0}
				<p class="muted">No skins found. k9s skins go in a <code>skins</code> folder next to the other files.</p>
			{/if}

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
								<td class="actions"><button onclick={() => runHotkey(hotkey)}>Run</button></td>
							</tr>
						{/each}
					</tbody>
				</table>
			{:else}
				<p class="muted">No hotkeys. In <code>hotkeys.yaml</code>, a hotkey opens a view with a key, like <code>Shift-0</code> for <code>:pods</code>.</p>
			{/if}

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
		{/if}
	</div>
</section>

{#snippet swatch(theme: SkinTheme | null | undefined)}
	{@const v = theme?.vars ?? {}}
	<span class="swatch" style:background={v['--bg'] ?? (theme?.scheme === 'light' ? '#f7f8fa' : '#0f131a')}>
		<span class="line" style:background={v['--text'] ?? (theme?.scheme === 'light' ? '#1b2230' : '#e4e8ef')}></span>
		<span class="line short" style:background={v['--accent'] ?? '#5b8def'}></span>
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
	}
	header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 16px;
		padding: 14px 20px 10px;
	}
	.title {
		display: flex;
		align-items: baseline;
		gap: 12px;
		min-width: 0;
	}
	h1 {
		margin: 0;
		font-size: 20px;
		white-space: nowrap;
	}
	h2 {
		display: flex;
		align-items: center;
		gap: 8px;
		margin: 28px 0 6px;
		font-size: 15px;
	}
	.scroll {
		flex: 1;
		overflow: auto;
		padding: 0 20px 24px;
	}
	p {
		margin: 6px 0;
	}
	.reload {
		padding: 5px 12px;
		border: 1px solid var(--border);
		border-radius: 6px;
		background: var(--surface-1);
		font-size: 12.5px;
	}
	.reload:hover:not(:disabled) {
		border-color: var(--accent);
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
		padding: 6px 10px;
		border-bottom: 1px solid var(--border);
		text-align: left;
		font-size: 11px;
		font-weight: 600;
		letter-spacing: 0.05em;
		text-transform: uppercase;
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
	.actions button {
		padding: 2px 10px;
		border: 1px solid var(--border);
		border-radius: 6px;
		background: var(--surface-1);
		font-size: 12px;
	}
	.actions button:hover {
		border-color: var(--accent);
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
</style>
