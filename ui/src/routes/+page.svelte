<script lang="ts">
	import { drafts } from '#lib/navigation.svelte.js';
	import { applyScheme } from '#lib/skin.js';
	import { dropdown } from '#lib/dismiss.js';
	import { t } from '#lib/i18n/index.js';
	import { coreText } from '#lib/i18n/errors.js';
	import { stepZoom, zoomKey } from '#lib/appearance.js';
	import Icon from '#lib/components/Icon.svelte';
	import Phrase from '#lib/components/Phrase.svelte';
	import { isTauri } from '@tauri-apps/api/core';
	import { untrack } from 'svelte';
	import { forControl } from '#lib/keys.js';
	import logo from '#lib/assets/logo.svg';
	import { canDelete, canEdit, canRestart, canScale } from '#lib/actions.js';
	import { listenForBindings } from '#lib/bindings.js';
	import ActionDialog from '#lib/components/ActionDialog.svelte';
	import ActionMenu from '#lib/components/ActionMenu.svelte';
	import ExplainPanel from '#lib/components/ExplainPanel.svelte';
	import BenchmarksView from '#lib/components/BenchmarksView.svelte';
	import CommandPalette from '#lib/components/CommandPalette.svelte';
	import ConfigView from '#lib/components/ConfigView.svelte';
	import ContextSettingsForm from '#lib/components/ContextSettingsForm.svelte';
	import ContextSwitcher from '#lib/components/ContextSwitcher.svelte';
	import ConnectCard from '#lib/components/ConnectCard.svelte';
	import CreateView from '#lib/components/CreateView.svelte';
	import DetailsPanel from '#lib/components/DetailsPanel.svelte';
	import Dock from '#lib/components/Dock.svelte';
	import DebugDialog from '#lib/components/DebugDialog.svelte';
	import CompareDialog from '#lib/components/CompareDialog.svelte';
	import GrafanaDialog from '#lib/components/GrafanaDialog.svelte';
	import ForbiddenDialog from '#lib/components/ForbiddenDialog.svelte';
	import UpdateDialog from '#lib/components/UpdateDialog.svelte';
	import AgentRequests from '#lib/components/AgentRequests.svelte';
	import HelmView from '#lib/components/HelmView.svelte';
	import KubectlDialog from '#lib/components/KubectlDialog.svelte';
	import SnapshotDialog from '#lib/components/SnapshotDialog.svelte';
	import KubeconfigFilesDialog from '#lib/components/KubeconfigFilesDialog.svelte';
	import ActivityPanel from '#lib/components/ActivityPanel.svelte';
	import HelpOverlay from '#lib/components/HelpOverlay.svelte';
	import Tour from '#lib/components/Tour.svelte';
	import NamespacePicker from '#lib/components/NamespacePicker.svelte';
	import PluginTerminal from '#lib/components/PluginTerminal.svelte';
	import LocalClusterDialog from '#lib/components/LocalClusterDialog.svelte';
	import { closeSnapshot, openDemo, retryKubeconfig } from '#lib/start.js';
	import PortForwardsView from '#lib/components/PortForwardsView.svelte';
	import PulsesView from '#lib/components/PulsesView.svelte';
	import RbacView from '#lib/components/RbacView.svelte';
	import ResourceView from '#lib/components/ResourceView.svelte';
	import SanitizerView from '#lib/components/SanitizerView.svelte';
	import ToolsView from '#lib/components/ToolsView.svelte';
	import KindsView from '#lib/components/KindsView.svelte';
	import IntegrationsView from '#lib/components/IntegrationsView.svelte';
	import RightSizingView from '#lib/components/RightSizingView.svelte';
	import FleetView from '#lib/components/FleetView.svelte';
	import SearchView from '#lib/components/SearchView.svelte';
	import GraphView from '#lib/components/GraphView.svelte';
	import ChangesView from '#lib/components/ChangesView.svelte';
	import Sidebar from '#lib/components/Sidebar.svelte';
	import { readOnlyFixed, readOnlyReason } from '#lib/guard.js';
	import Toasts from '#lib/components/Toasts.svelte';
	import XrayView from '#lib/components/XrayView.svelte';
	import { parseShortcut, pluginApplies, shortcutLabel } from '#lib/k9s.js';
	import { keyFromLabel } from '#lib/menu.js';
	import {
		canPortForward,
		hasImages,
		hasLogs,
		hasUsers,
		isCronJobs,
		isNamespaces,
		isNodes,
		isPods,
		isReplicaSets,
		isSecrets
	} from '#lib/pods.js';
	import { backend } from '#lib/api/index.js';
	import { age } from '#lib/format.js';
	import { dock } from '#lib/dock.svelte.js';
	import { session } from '#lib/session.svelte.js';
	import { app } from '#lib/state.svelte.js';
	import { LiveTable } from '#lib/watch.svelte.js';

	const namespaces = new LiveTable();
	/** The dialog that makes a cluster on this computer, from the page shown without a kubeconfig. */
	let localClusterOpen = $state(false);

	$effect(() => {
		// Also after signing in again, when a message from before may still be up.
		app.error = null;
		if (!isTauri()) untrack(() => app.readAddress(location.hash));
		app.load();
	});

	// In the browser, the address follows the view, so a reload opens it again.
	$effect(() => {
		const address = app.address;
		if (isTauri() || !address) return;
		untrack(() => {
			// Back left Kubri's history behind; going somewhere starts it again.
			if (history.state?.kubri === 'before' && app.canGoBack) history.pushState({ kubri: 'now' }, '', address);
			else if (location.hash !== address) history.replaceState(history.state, '', address);
		});
	});

	/** When the mouse's back button last went back, which some browsers also take as their Back. */
	let mouseBackAt = -Infinity;

	// The browser's Back goes back in Kubri, as `[` does, and leaves Kubri only from its first view.
	$effect(() => {
		if (isTauri()) return;
		if (history.state?.kubri !== 'now') {
			history.replaceState({ kubri: 'before' }, '');
			history.pushState({ kubri: 'now' }, '');
		}
		const onpopstate = (e: PopStateEvent) => {
			if (e.state?.kubri !== 'before') return;
			if (performance.now() - mouseBackAt < 1000) {
				// The mouse button went back already.
				history.pushState({ kubri: 'now' }, '', app.address ?? location.href);
			} else if (app.canGoBack) {
				app.go(-1);
				history.pushState({ kubri: 'now' }, '', app.address ?? location.href);
			} else {
				history.back();
			}
		};
		// An address typed or pasted in; Back to the first view is handled above.
		const onhashchange = () => history.state?.kubri !== 'before' && app.goToAddress(location.hash);
		addEventListener('popstate', onpopstate);
		addEventListener('hashchange', onhashchange);
		return () => {
			removeEventListener('popstate', onpopstate);
			removeEventListener('hashchange', onhashchange);
		};
	});

	$effect(() => untrack(() => applyScheme(app.scheme)));
	// The system's light or dark, followed when it changes, such as at sunset.
	$effect(() => {
		const media = matchMedia('(prefers-color-scheme: dark)');
		const update = () => (app.systemDark = media.matches);
		update();
		media.addEventListener('change', update);
		return () => media.removeEventListener('change', update);
	});
	const desktop = isTauri();
	// The level kept from the last start; the window opens at 100%.
	$effect(() =>
		untrack(() => {
			if (desktop && app.zoom !== 1) app.setZoom(app.zoom);
		})
	);

	// k9s plugin and hotkey keys.
	$effect(() => listenForBindings());

	// The quick tour, once, the first time there is a cluster to look at.
	$effect(() => {
		if (app.cluster) untrack(() => app.maybeStartTour());
	});

	$effect(() => {
		if (!app.cluster) return;
		return namespaces.start({ group: '', version: 'v1', plural: 'namespaces' }, null);
	});

	/** The context's settings name the namespaces to show, by list or by label. */
	const limited = $derived(app.contextSettings.namespaces.length > 0 || !!app.contextSettings.namespaceSelector.trim());

	$effect(() => {
		if (!limited) app.namespaces = namespaces.rows.map((r) => r.name).sort();
	});

	// Someone who may not list namespaces still gets the ones the kubeconfig and settings name.
	$effect(() => {
		if (!app.cluster || (!limited && (!namespaces.error || namespaces.rows.length))) return;
		// Read so that changing the limit asks again.
		void [app.contextSettings.namespaces, app.contextSettings.namespaceSelector];
		const context = app.cluster.context;
		backend.readableNamespaces().then(
			(names) => {
				if (app.cluster?.context === context && (limited || !namespaces.rows.length)) app.namespaces = names;
			},
			() => {}
		);
	});

	// Every place Kubri shows goes into the history, for `[`, `]` and `-`.
	$effect(() => {
		const view = app.view;
		untrack(() => app.remember(view));
	});

	// Port-forwards and benchmarks outlive context switches; keep the sidebar counts current.
	$effect(() => {
		app.refreshForwards();
		app.refreshBenchmarks();
		const timer = setInterval(() => {
			app.refreshForwards();
			app.refreshBenchmarks();
		}, 5000);
		return () => clearInterval(timer);
	});

	function onkeydown(e: KeyboardEvent) {
		const zoom = desktop ? zoomKey(e) : null;
		if (zoom !== null) {
			e.preventDefault();
			app.setZoom(stepZoom(app.zoom, zoom));
			return;
		}
		if (app.overlayOpen) return;
		// Ctrl+` (the key left of 1) opens the local terminal, or shows the one open; with Shift
		// another one.
		if ((e.ctrlKey || e.metaKey) && e.code === 'Backquote') {
			e.preventDefault();
			app.openLocalTerminal(e.shiftKey);
			return;
		}
		const typing = forControl(e);
		if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'k') {
			e.preventDefault();
			app.paletteOpen = true;
		} else if (!typing && e.key === ':') {
			e.preventDefault();
			app.paletteOpen = true;
		} else if (!typing && e.key === '?') {
			e.preventDefault();
			app.helpOpen = true;
		} else if (!typing && !e.ctrlKey && !e.metaKey && (e.key === '[' || e.key === ']' || e.key === '-')) {
			// Back, forward and the last view, as in k9s.
			e.preventDefault();
			if (e.key === '-') app.toggleLast();
			else app.go(e.key === '[' ? -1 : 1);
		} else if (e.altKey && (e.key === 'ArrowLeft' || e.key === 'ArrowRight')) {
			e.preventDefault();
			app.go(e.key === 'ArrowLeft' ? -1 : 1);
		}
	}

	// The back and forward buttons of a mouse.
	function onmouseup(e: MouseEvent) {
		if ((e.button !== 3 && e.button !== 4) || app.overlayOpen) return;
		e.preventDefault();
		if (e.button === 3) mouseBackAt = performance.now();
		app.go(e.button === 3 ? -1 : 1);
	}

	const viewHints = $derived.by(() => {
		// The view waits under the connecting card; only cancelling works.
		if (app.connecting && app.cluster && app.contexts.some((c) => c.name === app.connecting)) return [['esc', t('cancel')]] as [string, string][];
		const panel: [string, string][] = app.details ? [['f', app.maximized ? t('restore') : t('maximize')], ['esc', t('close')]] : [];
		// The log keys, while the logs are on screen.
		if (app.details?.tab === 'logs') {
			return [
				['/', t('search')],
				['n/N', t('match')],
				['0–6', t('range')],
				['t', t('timestamps')],
				['w', t('wrap')],
				['s', t('autoscroll')],
				['m', t('mark')],
				['x', t('explain')],
				['c', t('copy')],
				['^s', t('save')],
				...panel,
				['?', t('help')]
			] as [string, string][];
		}
		// Typing goes to the shell. The panel's keys work there as buttons, and after Esc twice.
		if (app.details?.tab === 'shell') {
			return [['esc esc', t('leave terminal')], ['ctrl+d', t('exit shell')], ...panel, ['?', t('help')]] as [string, string][];
		}
		if (app.page === 'portforwards') {
			return [
				[':', t('command')],
				['c', t('copy url')],
				['o', t('open')],
				['s', t('favorite')],
				['b', t('benchmark')],
				['^d', t('stop')],
				['?', t('help')]
			] as [string, string][];
		}
		if (app.page === 'pulses') {
			return [[':', t('command')], ['←→', t('move')], ['enter', t('open list')], ...panel, ['?', t('help')]] as [string, string][];
		}
		if (app.page === 'xray') {
			return [
				[':', t('command')],
				['/', t('filter')],
				['h/l', t('collapse/expand')],
				['space', t('toggle')],
				['enter', t('describe')],
				...panel,
				['?', t('help')]
			] as [string, string][];
		}
		if (app.page === 'kinds') {
			return [[':', t('command')], ['/', t('find')], ['j/k', t('move')], ['enter', t('open')], ['?', t('help')]] as [string, string][];
		}
		if (app.page === 'rbac') {
			return [[':', t('command')], ['/', t('filter')], ['j/k', t('move')], ...panel, ['?', t('help')]] as [string, string][];
		}
		if (app.page === 'sanitizer') {
			return [
				[':', t('command')],
				['/', t('filter')],
				['h/l', t('collapse/expand')],
				['enter', t('describe')],
				['⇧e', t('explain')],
				['⇧r', t('scan again')],
				...panel,
				['?', t('help')]
			] as [string, string][];
		}
		if (app.page === 'create') {
			return [[':', t('command')], ['^s', t('review')], ['esc', t('back to editing')], ['?', t('help')]] as [string, string][];
		}
		if (app.page === 'config') {
			return [[':', t('command')], ['⇧r', t('reload')], ['?', t('help')]] as [string, string][];
		}
		if (app.page === 'benchmarks') {
			return [[':', t('command')], ['j/k', t('move')], ['b', t('run again')], ['^d', t('stop/remove')], ['?', t('help')]] as [string, string][];
		}
		if (app.page === 'helm') {
			return [
				[':', t('command')],
				['/', t('filter')],
				['enter', t('open')],
				['v', t('values')],
				['h', t('history')],
				['c', t('compare')],
				['x', t('drift')],
				['u', t('upgrade')],
				['i', t('install')],
				['r', t('roll back')],
				['^d', t('uninstall')],
				...panel,
				['?', t('help')]
			] as [string, string][];
		}
		// Pages without list keys of their own; the list's keys would do nothing there.
		if (app.page !== 'resources') return [[':', t('command')], ...panel, ['?', t('help')]] as [string, string][];
		const r = app.selected;
		const h: [string, string][] = [
			[':', t('command')],
			['/', t('filter')],
			['enter', t('open')],
			['d', t('describe')],
			['y', t('yaml')]
		];
		if (!r) return h;
		// An empty list has no row for the keys to act on.
		if (app.listRows === 0) {
			const empty: [string, string][] = [[':', t('command')], ['/', t('filter')]];
			if (app.scope) empty.push(['esc', t('back')]);
			if (app.details) empty.push(['f', app.maximized ? t('restore') : t('maximize')]);
			empty.push(['?', t('help')]);
			return empty;
		}
		const writable = !app.readOnly;
		if (hasLogs(r)) h.push(['l', t('logs')]);
		if (isPods(r) && writable) h.push(['s', t('shell')], ['a', t('attach')]);
		if (isPods(r)) h.push(['o', t('node')]);
		if (isSecrets(r)) h.push(['x', t('decode')]);
		if (isNamespaces(r)) h.push(['u', t('use')]);
		if (hasUsers(r)) h.push(['u', t('used by')]);
		if (canPortForward(r)) h.push(['⇧f', t('port-forward')]);
		if (isNodes(r) && writable) h.push(['c', t('cordon')], ['u', t('uncordon')], ['r', t('drain')]);
		if (isCronJobs(r) && writable) h.push(['t', t('trigger')], ['s', t('suspend')]);
		if (hasImages(r) && writable) h.push(['i', t('image')]);
		if (isReplicaSets(r) && writable) h.push(['^l', t('roll back')]);
		if (canEdit(r) && writable) h.push(['e', t('edit')]);
		if (canDelete(r) && writable) h.push(['^d', t('delete')]);
		if (canScale(r) && !isPods(r) && writable) h.push(['s', t('scale')]);
		if (canRestart(r) && writable) h.push(['r', t('restart')]);
		if (app.scope) h.push(['esc', t('back')]);
		if (app.details) h.push(['f', app.maximized ? t('restore') : t('maximize')]);
		h.push(['?', t('help')]);
		return h;
	});

	// The view's plugins, as k9s lists them in its menu; a key the view uses stays the view's.
	const pluginHints = $derived.by(() => {
		const taken = new Set(viewHints.map(([key]) => key));
		return (app.k9s?.plugins ?? [])
			.filter((p) => pluginApplies(p, app.pluginView))
			.flatMap((p): [string, string][] => {
				const shortcut = parseShortcut(p.shortCut);
				if (!shortcut) return [];
				const key = shortcutLabel(shortcut);
				return p.override || !taken.has(key) ? [[key, p.name]] : [];
			})
			.slice(0, 4);
	});

	/** Plays a hint's key, so that the hints work as buttons too. */
	function press(key: string) {
		const init = keyFromLabel(key);
		if (init) window.dispatchEvent(new KeyboardEvent('keydown', { ...init, bubbles: true, cancelable: true }));
	}

	/** Keeps the focus where it is, so that the keys work on the list after a click. */
	const keepFocus = (e: MouseEvent) => e.preventDefault();

	// The footer shows the keys that matter most; `?` lists them all. Esc and help always stay.
	const MAX_HINTS = 8;
	const shownHints = $derived.by(() => {
		if (viewHints.length <= MAX_HINTS) return viewHints;
		const keep = (hint: [string, string]) => hint[0] === 'esc' || hint[0] === '?';
		let room = MAX_HINTS - viewHints.filter(keep).length;
		return viewHints.filter((hint) => keep(hint) || room-- > 0);
	});

	// Help stays last.
	const hints = $derived([...shownHints.slice(0, -1), ...pluginHints, ...shownHints.slice(-1)]);

	function signOut() {
		if (!drafts.block(() => session.signOut())) session.signOut();
	}

	// A menu belongs to the view it opened in: going elsewhere, also with Back, closes it.
	$effect(() => {
		void app.view;
		void app.cluster?.context;
		untrack(() => (app.menu = null));
	});

	// A clock for the unlock countdown.
	let now = $state(Date.now());
	$effect(() => {
		if (app.unlockedUntil === null) return;
		const timer = setInterval(() => (now = Date.now()), 1000);
		return () => clearInterval(timer);
	});
	const unlockLeft = $derived(app.unlockedUntil === null ? null : Math.max(0, Math.ceil((app.unlockedUntil - now) / 1000)));
	const lockFixed = $derived(readOnlyFixed(app.guard, app.k9s?.readOnly ?? false, app.contextSettings.readOnly));
	const lockTitle = $derived(
		lockFixed
			? `${readOnlyReason(app.guard, app.contextSettings.readOnly, app.k9s?.readOnly ?? false)}.`
			: app.contextSettings.readOnly
				? t('Read-only in Kubri: no edits, deletes, scaling or shells. Click to allow changes.')
				: unlockLeft !== null
					? t('Unlocked for now; it locks again by itself. Click to lock it now.')
					: app.guard.readOnly && app.guard.canUnlock
						? t('Locked: click to unlock for 15 minutes.')
						: app.guard.readOnly
							? t('Read-only: {reason}', { reason: app.guard.reason ? coreText(app.guard.reason) : t('Kubri does not change this cluster') })
							: t('Changes are allowed; Kubernetes permissions still apply. Click to make Kubri read-only.')
	);
	const accent = $derived(app.contextSettings.color || null);

	/** In a narrow window the sidebar folds away behind a button, and folds again once a page is picked. */
	let navOpen = $state(false);
	$effect(() => {
		void app.page;
		void app.selected;
		navOpen = false;
	});
</script>

<svelte:window {onkeydown} {onmouseup} />

<div
	class="shell"
	class:maximized={app.details && app.maximized}
	class:inspecting={app.details}
	class:tinted={accent}
	class:nav-open={navOpen}
	style:--context-color={accent}
	style:--sidebar-width={app.sidebarWidth ? `${app.sidebarWidth}px` : null}
>
	<Sidebar />
	<div class="workspace">
		<header>
			<div class="brand">
				<img src={logo} alt="" width="24" height="24" />
				<span>kubri</span>
			</div>
			<button
				class="square menu-button"
				onclick={() => (navOpen = !navOpen)}
				aria-expanded={navOpen}
				title={navOpen ? t('Hide the resource list') : t('Show the resource list')}
				aria-label={t('Resources')}
			>
				<Icon name={navOpen ? 'close' : 'menu'} />
			</button>
			<div class="history">
				<button
					onclick={() => app.go(-1)}
					onmousedown={keepFocus}
					disabled={!app.canGoBack}
					title="{t('Back')} ([ {t('or')} Alt+←)"
					aria-label={t('Back')}
				>
					<Icon name="back" size={16} />
				</button>
				<button
					onclick={() => app.go(1)}
					onmousedown={keepFocus}
					disabled={!app.canGoForward}
					title="{t('Forward')} (] {t('or')} Alt+→)"
					aria-label={t('Forward')}
				>
					<Icon name="next" size={16} />
				</button>
			</div>
			<ContextSwitcher />
			{#if app.cluster && !app.snapshot}
				<details class="environment" {@attach dropdown}>
					<summary
						class:set={app.environment}
						class:production={app.production}
						title={t('Settings for this context: name, color and guard rails')}
						aria-label="{t('Context settings')}{app.environment ? `, ${app.environment}` : ''}"
					>
						<Icon name="sliders" size={16} />
						{#if app.environment}{app.environment}{/if}
					</summary>
					<div class="popover">
						<p class="popover-title">{t('Settings for')} <strong>{app.cluster.context}</strong></p>
						<ContextSettingsForm context={app.cluster.context} namespaces={app.namespaces} />
						<p class="note">
							{t("Kept in Kubri's settings file; the kubeconfig is never changed.")}
							<button
								class="all-contexts"
								onclick={(e) => {
									e.currentTarget.closest('details')?.removeAttribute('open');
									app.showConfig('contexts');
								}}
							>
								{t('All contexts in Settings')}
							</button>
						</p>
					</div>
				</details>
			{/if}
			<span class="divider"></span>
			<NamespacePicker />
			<button class="palette-button" data-tour="palette" onclick={() => (app.paletteOpen = true)} aria-label={t('Search or run a command')}>
				<Icon name="search" size={17} />
				<span>{t('Search or run a command')}</span>
				<kbd>Ctrl K</kbd>
			</button>
			{#if app.localTerminal && app.cluster && !app.snapshot}
				<button
					class="square"
					onclick={(e) => app.openLocalTerminal(e.shiftKey)}
					title={t('Terminal with kubectl and helm set to this context and namespace (Ctrl+`, with Shift another one)')}
					aria-label={t('Terminal')}
				>
					<Icon name="terminal" size={17} />
				</button>
			{/if}
			<button
				class="square"
				onclick={() => app.toggleScheme()}
				title={app.skin
					? app.dark
						? t("The skin {skin} sets the colors; use Kubri's light colors instead", { skin: app.skin })
						: t("The skin {skin} sets the colors; use Kubri's dark colors instead", { skin: app.skin })
					: app.dark
						? t('Use light colors')
						: t('Use dark colors')}
				aria-label={app.dark ? t('Use light colors') : t('Use dark colors')}
			>
				<Icon name={app.dark ? 'sun' : 'moon'} />
			</button>
			<button
				class="mode"
				data-tour="read-only"
				class:locked={app.readOnly}
				class:unlocked={app.unlockedUntil !== null}
				role="switch"
				aria-checked={app.readOnly}
				aria-label={t('Read-only')}
				disabled={!app.cluster || lockFixed}
				onclick={() => app.toggleReadOnly()}
				title={lockTitle}
			>
				<Icon name={app.readOnly ? 'lock' : 'unlock'} size={15} />
				{t('Read-only')}{unlockLeft !== null ? ` · ${Math.floor(unlockLeft / 60)}:${String(unlockLeft % 60).padStart(2, '0')}` : ''}
				<span class="track" aria-hidden="true"><span class="knob"></span></span>
			</button>
			<button class="square help" onclick={() => (app.helpOpen = true)} title="{t('Keys and commands')} (?)" aria-label={t('Help')}>
				?
			</button>
			{#if session.required}
				<button class="sign-out" onclick={signOut} title={session.person ? t('Signed in as {name}', { name: session.person.name }) : undefined}>{t('Sign out')}</button>
			{/if}
		</header>

		{#if app.cluster && app.production}
			<div class="production-banner" role="note">
				<Icon name="warning" size={15} />
				<span>
					<strong>{t('Production')}</strong> · {app.contextName(app.cluster.context)} ·
					{app.readOnly ? t('changes are off') : t('changes go live; deleting asks you to type the name')}
				</span>
			</div>
		{:else if app.cluster && app.productionNamespace}
			<div class="production-banner" role="note">
				<Icon name="warning" size={15} />
				<span>
					<strong>{t('Production namespace')}</strong> · {app.namespace} ·
					{app.readOnly ? t('changes are off') : t('changes go live; deleting asks you to type the name')}
				</span>
			</div>
		{/if}
		{#if app.snapshot}
			{@const snapshot = app.snapshot}
			<div class="snapshot-banner" role="note">
				<Icon name="camera" size={15} />
				<span>
					<strong>{t('Snapshot')}</strong> {t('of')} {snapshot.context} · {snapshot.namespace ? t('namespace {name}', { name: snapshot.namespace }) : t('the whole cluster')}
					· {t('taken {when} ({age} ago)', { when: new Date(snapshot.taken).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' }), age: age(new Date(snapshot.taken).toISOString(), Date.now()) || '0s' })}
					· {t('read-only, nothing here is live')}
					{#if snapshot.leftOut.length}
						· <span class="left-out" title={snapshot.leftOut.join('\n')}>{t('{n} left out', { n: snapshot.leftOut.length })}</span>
					{/if}
				</span>
				<button onclick={() => (app.contexts.length ? app.leaveSnapshot() : closeSnapshot())}>{t('Leave snapshot')}</button>
			</div>
		{/if}
		{#if app.settingsError}
			<div class="error" role="alert">
				<span>{t("Kubri's settings file could not be read, so it uses the defaults: {error}", { error: app.settingsError })}</span>
				<button onclick={() => (app.settingsError = null)} aria-label={t('Dismiss')}>×</button>
			</div>
		{/if}
		{#if app.error}
			<div class="error" role="alert">
				<span>{app.error}</span>
				<button onclick={() => (app.error = null)} aria-label={t('Dismiss')}>×</button>
			</div>
		{/if}

		<div class="area" class:right={dock.sessions.length && dock.side === 'right' && !dock.minimized}>
			{#if app.cluster && (app.connecting || app.connectFailure)}
				<!-- Over the view Kubri stays on: it is still there if connecting fails or is cancelled. -->
				<ConnectCard />
			{/if}
			<main
				inert={!!(app.cluster && app.connecting)}
				class:waiting={app.cluster && app.connecting}
				class:with-details={app.details}
				class:wide={app.panelWide}
				style:--panel-width={app.panelWidth ? `${app.panelWidth}px` : null}
			>
				{#if app.page === 'fleet'}
					<FleetView />
				{:else if app.cluster}
					{#if app.page === 'portforwards'}
						<PortForwardsView />
					{:else if app.page === 'benchmarks'}
						<BenchmarksView />
					{:else if app.page === 'search'}
						<SearchView />
					{:else if app.page === 'config'}
						<ConfigView />
					{:else if app.page === 'integrations'}
						{#key app.cluster?.context}
							<IntegrationsView />
						{/key}
					{:else if app.page === 'tools'}
						<ToolsView />
					{:else if app.page === 'kinds'}
						{#key app.cluster?.context}
							<KindsView />
						{/key}
					{:else if app.page === 'rightsizing'}
						{#key `${app.cluster?.context}/${app.namespace}`}
							<RightSizingView />
						{/key}
					{:else if app.page === 'sanitizer'}
						{#key `${app.cluster?.context}/${app.namespace}`}
							<SanitizerView />
						{/key}
					{:else if app.page === 'pulses'}
						{#if !(app.starting && app.cluster)}
							{#key `${app.cluster?.context}/${app.namespace}`}
								<PulsesView />
							{/key}
						{/if}
					{:else if app.page === 'graph'}
						{#key `${app.cluster?.context}/${app.namespace}`}
							<GraphView />
						{/key}
					{:else if app.page === 'changes'}
						{#key `${app.cluster?.context}/${app.namespace}/${JSON.stringify(app.changesFilter)}`}
							<ChangesView />
						{/key}
					{:else if app.page === 'xray' && app.xrayResource}
						{#key `${app.cluster?.context}/${app.namespace}/${app.xrayResource.plural}`}
							<XrayView resource={app.xrayResource} />
						{/key}
					{:else if app.page === 'rbac'}
						{#key `${app.cluster?.context}/${app.namespace}`}
							<RbacView />
						{/key}
					{:else if app.page === 'create'}
					<CreateView />
				{:else if app.page === 'helm'}
						{#key `${app.cluster?.context}/${app.namespace}`}
							<HelmView />
						{/key}
					{:else if app.selected}
						{#key `${app.cluster?.context}/${app.selected.group}/${app.selected.version}/${app.selected.plural}/${JSON.stringify(app.scope)}`}
							<ResourceView resource={app.selected} namespace={app.namespace} scope={app.scope} />
						{/key}
					{:else}
						<div class="placeholder"><Phrase text={t('Choose a resource or press {key}.')} kbd={{ key: ':' }} /></div>
					{/if}
				{:else}
					{#if app.kubeconfigProblem !== null && !app.contexts.length && !app.connecting}
						<section class="connect">
							<img src={logo} alt="" width="56" height="56" />
							<span class="eyebrow">{t('Getting started')}</span>
							<h1>{t('No clusters yet.')}</h1>
							<p>
								{isTauri()
									? t('Kubri reads your clusters from a kubeconfig, as kubectl does, and found none it can use. Load the kubeconfig file you got for your cluster, or start a cluster on this computer.')
									: t('kubri-server found no kubeconfig it can use. Add contexts from a kubeconfig file, or start the server with one.')}
								{t('To look around first, open the demo: a snapshot of a small shop, read-only.')}
							</p>
							<p class="problem">{app.kubeconfigProblem}</p>
							<div class="connect-actions">
								<button class="primary" onclick={() => (app.kubeconfigFilesOpen = isTauri() ? true : 'copy')}>
									{isTauri() ? t('Load kubeconfig file…') : t('Add contexts from a file…')}
									<Icon name="arrow" size={16} />
								</button>
								{#await backend.localClusters().catch(() => null) then clusters}
									{#if clusters?.allowed}<button onclick={() => (localClusterOpen = true)}>{t('Start a cluster…')}</button>{/if}
								{/await}
								<button onclick={openDemo}>{t('Open the demo')}</button>
								<button onclick={retryKubeconfig}>{t('Try again')}</button>
							</div>
							{#if localClusterOpen}<LocalClusterDialog onclose={() => (localClusterOpen = false)} />{/if}
							<p class="small">
								{t('Kubri looks in the files listed in')} <code>KUBECONFIG</code>{t(', or in')} <code>~/.kube/config</code>.
							</p>
						</section>
					{:else if app.connecting || app.connectFailure}
						<section class="connect">
							<img src={logo} alt="" width="56" height="56" />
							<ConnectCard inline />
							{#if !app.connecting}
								<div class="connect-actions">
									<button onclick={() => (app.kubeconfigFilesOpen = true)}>{t('Kubeconfig files…')}</button>
								</div>
							{/if}
						</section>
					{:else if app.loaded}
						<section class="connect">
							<img src={logo} alt="" width="56" height="56" />
							<h1>{t('Connect to a cluster.')}</h1>
							<p>
								{isTauri()
									? t('Choose a context from your kubeconfig. Reload it after you update your credentials.')
									: t('Choose a context configured on kubri-server. The server holds the credentials.')}
							</p>
							<div class="connect-actions">
								<button
									class="primary"
									onclick={() => {
										app.contextMenuOpen = true;
										app.reloadContexts(true);
									}}
								>
									{t('Choose context')} <Icon name="arrow" size={16} />
								</button>
								<button onclick={() => app.load()}>{t('Try again')}</button>
								<button onclick={() => (app.kubeconfigFilesOpen = true)}>{t('Kubeconfig files…')}</button>
								{#if app.contexts.length > 1}<button onclick={() => app.showFleet()}>{t('See all clusters')}</button>{/if}
							</div>
							<p class="small">{t('Your k9s aliases, hotkeys, plugins and skins keep working.')}</p>
						</section>
					{/if}
				{/if}
				{#if app.details}
					<DetailsPanel details={app.details} />
				{/if}
			</main>
			{#if dock.sessions.length}<Dock />{/if}
			{#if app.activityOpen}<ActivityPanel />{/if}
		</div>

		<footer>
			<span class="status" class:live={app.cluster && !app.connecting && !app.snapshot} role="status">
				<span class="dot"></span>
				{app.connectingText
					? app.connectingText
					: app.snapshot
						? t('Snapshot · {name}', { name: app.snapshot.file ?? app.snapshot.context })
						: app.cluster
							? t('Live · {name}', { name: app.contextName(app.cluster.context) })
							: t('Not connected')}
			</span>
			{#if app.release}
				{@const release = app.release}
				<span class="release">
					{#if release.installable}
						<button onclick={() => (app.updateOpen = true)} title={t('Update Kubri to {version}', { version: release.version })}>
							{t('Kubri {version} is out', { version: release.version })} · {t('Update')}
						</button>
					{:else}
						<button
							onclick={() => backend.openUrl(release.url)}
							title={release.manual ? `${release.manual} ${t('Opens the release notes and downloads.')}` : t('Open the release notes and downloads')}
						>
							{t('Kubri {version} is out', { version: release.version })}
						</button>
					{/if}
					<button class="close" onclick={() => app.dismissRelease()} aria-label={t('Hide until the next release')}>×</button>
				</span>
			{/if}
			<span class="hints" data-tour="keys">
				{#each hints as [key, label], i (key + i)}
					{#if keyFromLabel(key)}
						<button onclick={() => press(key)} onmousedown={keepFocus} title="{label} ({key})"><kbd>{key}</kbd>{label}</button>
					{:else}
						<span><kbd>{key}</kbd>{label}</span>
					{/if}
				{/each}
			</span>
		</footer>
	</div>
</div>

{#if app.paletteOpen}<CommandPalette />{/if}
{#if app.helpOpen}<HelpOverlay />{/if}
{#if app.tourOpen}<Tour />{/if}
{#if app.menu}<ActionMenu menu={app.menu} />{/if}
<AgentRequests />
{#if app.dialog?.kind === 'forbidden'}
	<ForbiddenDialog dialog={app.dialog} />
{:else if app.dialog?.kind === 'kubectl'}
	<KubectlDialog dialog={app.dialog} />
{:else if app.dialog?.kind === 'debug'}
	<DebugDialog dialog={app.dialog} />
{:else if app.dialog?.kind === 'compare'}
	<CompareDialog dialog={app.dialog} />
{:else if app.dialog?.kind === 'grafana'}
	<GrafanaDialog dialog={app.dialog} />
{:else if app.dialog?.kind === 'snapshot'}
	<SnapshotDialog dialog={app.dialog} />
{:else if app.dialog}
	<ActionDialog dialog={app.dialog} />
{/if}
{#if app.kubeconfigFilesOpen}<KubeconfigFilesDialog />{/if}
{#if app.updateOpen && app.release}<UpdateDialog release={app.release} />{/if}
{#if app.explaining}<ExplainPanel subject={app.explaining} />{/if}
{#if app.pluginTerminal}
	<PluginTerminal plugin={app.pluginTerminal.plugin} run={app.pluginTerminal.run} />
{/if}
<Toasts />

<style>
	.shell {
		display: grid;
		/* The sidebar can be dragged wider, yet never takes more than a third of the window. */
		grid-template-columns: max(220px, min(var(--sidebar-width, 220px), 33vw)) minmax(0, 1fr);
		height: 100dvh;
	}
	/* Logs and shells can take the whole window. */
	.shell.maximized {
		grid-template-columns: minmax(0, 1fr);
	}
	.shell.maximized > :global(nav) {
		display: none;
	}
	.workspace {
		display: flex;
		flex-direction: column;
		min-width: 0;
		min-height: 0;
	}
	header {
		display: flex;
		align-items: center;
		gap: 10px;
		min-height: 64px;
		padding: 8px 20px;
		border-bottom: 1px solid var(--border);
		background: var(--surface-1);
	}
	/* The sidebar shows the brand; the header takes it over when the sidebar is hidden. */
	.brand {
		display: none;
		align-items: center;
		gap: 7px;
		font-size: 21px;
		font-weight: 700;
		letter-spacing: -0.045em;
	}
	.history {
		display: flex;
		gap: 2px;
	}
	.history button,
	.square {
		display: grid;
		place-items: center;
		width: 34px;
		height: 34px;
		flex: none;
		padding: 0;
		border: 1px solid transparent;
		border-radius: 8px;
		background: none;
		color: var(--text);
	}
	.history button:hover:not(:disabled),
	.square:hover:not(:disabled) {
		background: var(--surface-2);
	}
	.square {
		border-color: var(--border);
	}
	.help {
		display: none;
		font-weight: 700;
	}
	.menu-button {
		display: none;
	}
	.environment {
		position: relative;
		flex: none;
	}
	.environment summary {
		display: flex;
		align-items: center;
		gap: 5px;
		padding: 6px 8px;
		border: 1px solid transparent;
		border-radius: 7px;
		font-size: 10.5px;
		font-weight: 700;
		letter-spacing: 0.06em;
		text-transform: uppercase;
		list-style: none;
		cursor: pointer;
		color: var(--text-muted);
	}
	.environment summary::-webkit-details-marker {
		display: none;
	}
	.environment summary:hover {
		border-color: var(--border);
		color: var(--text);
	}
	.environment summary.set {
		background: color-mix(in srgb, var(--warn) 16%, var(--surface-1));
		color: var(--warn);
	}
	.environment summary.production {
		background: color-mix(in srgb, var(--bad) 16%, var(--surface-1));
		color: var(--bad);
	}
	.environment .popover {
		position: absolute;
		top: calc(100% + 8px);
		left: 0;
		z-index: 20;
		display: grid;
		gap: 12px;
		width: 320px;
		max-height: calc(100dvh - 100px);
		overflow: auto;
		padding: 14px;
		border: 1px solid var(--border);
		border-radius: 10px;
		background: var(--surface-1);
		box-shadow: var(--shadow);
	}
	.popover-title {
		margin: 0;
		font-size: 13px;
	}
	.environment .note {
		margin: 0;
		color: var(--text-muted);
		font-size: 12px;
	}
	.environment .all-contexts {
		display: block;
		margin-top: 6px;
		padding: 0;
		border: none;
		background: none;
		color: var(--accent-strong);
		font-size: 12px;
		font-weight: 600;
	}
	.environment .all-contexts:hover {
		text-decoration: underline;
	}
	/* The context's color, as a strip over the header. */
	.shell.tinted header {
		box-shadow: inset 0 3px 0 var(--context-color);
		background: color-mix(in srgb, var(--context-color) 6%, var(--surface-1));
	}
	.production-banner {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 6px 20px;
		background: color-mix(in srgb, var(--bad) 14%, var(--surface-1));
		color: var(--bad);
		font-size: 12.5px;
	}
	.snapshot-banner {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 5px 12px 5px 20px;
		background: color-mix(in srgb, var(--accent) 12%, var(--surface-1));
		color: var(--text);
		font-size: 12.5px;
	}
	.snapshot-banner span {
		flex: 1;
		min-width: 0;
	}
	.snapshot-banner .left-out {
		text-decoration: underline dotted;
		cursor: help;
	}
	.snapshot-banner button {
		flex: none;
		padding: 3px 10px;
		border: 1px solid var(--border);
		border-radius: 6px;
		background: var(--surface-1);
		font-size: 12px;
	}
	.mode.unlocked {
		border-color: transparent;
		background: color-mix(in srgb, var(--warn) 16%, var(--surface-1));
		color: var(--warn);
	}
	.divider {
		width: 1px;
		height: 26px;
		flex: none;
		background: var(--border);
	}
	.palette-button {
		display: flex;
		align-items: center;
		gap: 10px;
		margin-left: auto;
		width: 300px;
		min-width: 0;
		padding: 7px 8px 7px 12px;
		border: 1px solid var(--border);
		border-radius: 8px;
		background: var(--bg);
		color: var(--text-muted);
		font-size: 13px;
	}
	.palette-button span {
		flex: 1;
		overflow: hidden;
		text-align: left;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.palette-button:hover {
		border-color: var(--accent);
	}
	.palette-button kbd {
		margin: 0;
	}
	.mode {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		flex: none;
		padding: 6px 10px;
		border: 1px solid var(--border);
		border-radius: 7px;
		background: var(--surface-1);
		font-size: 12px;
		font-weight: 600;
		color: var(--text-muted);
		white-space: nowrap;
	}
	.track {
		position: relative;
		width: 26px;
		height: 14px;
		border-radius: 999px;
		background: var(--border);
		transition: background 0.12s;
	}
	.knob {
		position: absolute;
		top: 2px;
		left: 2px;
		width: 10px;
		height: 10px;
		border-radius: 50%;
		background: var(--surface-1);
		transition: transform 0.12s;
	}
	.mode.locked .track {
		background: var(--accent);
	}
	.mode.locked .knob {
		transform: translateX(12px);
	}
	.mode.locked {
		border-color: transparent;
		background: color-mix(in srgb, var(--accent) 14%, var(--surface-1));
		color: var(--accent-strong);
	}
	.mode:hover:not(:disabled) {
		border-color: var(--accent);
	}
	.sign-out {
		flex: none;
		padding: 6px 10px;
		border: 1px solid var(--border);
		border-radius: 7px;
		background: none;
		color: var(--text-muted);
		font-size: 12px;
	}
	.sign-out:hover {
		border-color: var(--accent);
		color: var(--text);
	}
	.error {
		display: flex;
		justify-content: space-between;
		gap: 12px;
		padding: 8px 20px;
		background: color-mix(in srgb, var(--bad) 14%, var(--surface-1));
		color: var(--bad);
	}
	.error button {
		border: none;
		background: none;
		font-size: 18px;
		line-height: 1;
	}
	/* The page, and the workspace below or beside it. */
	.area {
		position: relative;
		display: flex;
		flex-direction: column;
		flex: 1;
		min-width: 0;
		min-height: 0;
	}
	main.waiting {
		opacity: 0.45;
		filter: saturate(0.6);
		transition: opacity 0.2s;
	}
	.area.right {
		flex-direction: row;
	}
	main {
		flex: 1;
		min-width: 0;
		display: grid;
		grid-template-columns: minmax(0, 1fr);
		min-height: 0;
	}
	/* A dragged width never leaves the list less than 320px. */
	main.with-details {
		grid-template-columns: minmax(0, 1fr) min(var(--panel-width, clamp(420px, 42vw, 760px)), calc(100% - 320px));
	}
	/* The list stays mounted so its shortcuts keep working. */
	main.wide {
		grid-template-columns: minmax(0, 1fr);
	}
	main.wide > :global(:not(aside)) {
		display: none;
	}
	.placeholder {
		display: grid;
		place-items: center;
		color: var(--text-muted);
	}
	.connect {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 10px;
		padding: 32px 20px;
		text-align: center;
		overflow: auto;
	}
	.eyebrow {
		margin-top: 10px;
		font-size: 11px;
		font-weight: 650;
		letter-spacing: 0.12em;
		text-transform: uppercase;
		color: var(--text-muted);
	}
	.connect h1 {
		margin: 0;
		font-size: 32px;
		letter-spacing: -0.03em;
	}
	.connect p {
		max-width: 460px;
		margin: 0;
		color: var(--text-muted);
		line-height: 1.6;
	}
	.connect .problem {
		max-width: 560px;
		padding: 8px 12px;
		border-radius: 8px;
		background: color-mix(in srgb, var(--bad) 8%, transparent);
		color: var(--bad);
		font-size: 13px;
		overflow-wrap: anywhere;
	}
	.connect .small {
		margin-top: 18px;
		font-size: 12px;
	}
	.connect-actions {
		display: flex;
		flex-wrap: wrap;
		justify-content: center;
		gap: 8px;
		margin-top: 14px;
	}
	.connect-actions button {
		display: inline-flex;
		align-items: center;
		gap: 8px;
		padding: 9px 16px;
		border: 1px solid var(--border);
		border-radius: 8px;
		background: var(--surface-1);
		font-weight: 600;
	}
	.connect-actions .primary {
		border-color: var(--accent);
		background: var(--accent);
		color: var(--on-accent);
	}
	footer {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 4px 16px;
		border-top: 1px solid var(--border);
		background: var(--surface-1);
		font-size: 12px;
		color: var(--text-muted);
	}
	.status {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		flex: none;
		white-space: nowrap;
	}
	.release {
		display: inline-flex;
		align-items: center;
		flex: none;
		border: 1px solid color-mix(in srgb, var(--accent) 40%, var(--border));
		border-radius: 999px;
		background: color-mix(in srgb, var(--accent) 8%, var(--surface-1));
	}
	.release button {
		padding: 1px 4px 1px 10px;
		border: none;
		background: none;
		color: var(--accent);
		font: inherit;
		font-weight: 600;
		cursor: pointer;
	}
	.release .close {
		padding: 1px 9px 1px 4px;
		color: var(--text-muted);
		font-weight: 400;
	}
	.status .dot {
		width: 7px;
		height: 7px;
		border-radius: 50%;
		background: var(--text-muted);
	}
	.status.live {
		color: var(--ok);
	}
	.status.live .dot {
		background: var(--ok);
	}
	.hints {
		display: flex;
		flex-wrap: wrap;
		justify-content: flex-end;
		gap: 2px 4px;
		margin-left: auto;
		min-width: 0;
	}
	.hints > * {
		display: inline-flex;
		align-items: center;
		padding: 2px 6px;
		border: 1px solid transparent;
		border-radius: 6px;
		background: none;
		color: inherit;
		font-size: 12px;
	}
	.hints > button:hover {
		border-color: var(--border);
		background: var(--surface-2);
		color: var(--text);
	}
	kbd,
	.placeholder :global(kbd) {
		margin-right: 5px;
		padding: 0 5px;
		border: 1px solid var(--border);
		border-radius: 4px;
		background: var(--surface-2);
		font-family: var(--mono);
		font-size: 11px;
		color: var(--text);
	}

	@media (max-width: 1280px) {
		.palette-button {
			width: 36px;
			flex: none;
			justify-content: center;
			padding: 7px;
		}
		.palette-button span,
		.palette-button kbd {
			display: none;
		}
	}
	/* A small laptop window keeps the labels in a narrower sidebar. */
	@media (min-width: 901px) and (max-width: 1100px) {
		.shell {
			grid-template-columns: 184px minmax(0, 1fr);
		}
	}
	/* Below that the sidebar is a rail of icons; the labels are in the tooltips. */
	@media (min-width: 721px) and (max-width: 900px) {
		.shell {
			grid-template-columns: 56px minmax(0, 1fr);
		}
	}
	@media (min-width: 721px) and (max-width: 1100px) {
		.mode {
			font-size: 0;
			gap: 0;
		}
	}
	/* The inspector takes the place of the list and the sidebar. */
	@media (max-width: 1100px) {
		.shell.inspecting {
			grid-template-columns: minmax(0, 1fr);
		}
		.shell.inspecting > :global(nav) {
			display: none;
		}
		.shell.inspecting .brand {
			display: flex;
		}
		main.with-details {
			grid-template-columns: minmax(0, 1fr);
		}
		main.with-details > :global(:not(aside)) {
			display: none;
		}
	}
	@media (max-width: 900px) {
		.history,
		.divider {
			display: none;
		}
		/* Keys beyond the first few stay in the help. */
		.hints > :nth-child(n + 6):not(:last-child) {
			display: none;
		}
	}
	/* In a short window, as at 200% zoom, the page scrolls: the header moves away and the page
	   below it gets the whole height, so lists keep room for their rows. */
	@media (max-height: 600px) {
		.shell {
			height: auto;
			min-height: 100dvh;
		}
		main {
			flex: none;
			height: 100dvh;
		}
	}
	/* One column: the header, then the sidebar as a strip of links, then the page. */
	@media (max-width: 720px) {
		.shell {
			display: flex;
			flex-direction: column;
		}
		.workspace {
			display: contents;
		}
		.shell > :global(nav) {
			order: 1;
			display: none;
		}
		.shell.nav-open > :global(nav) {
			display: flex;
		}
		.menu-button {
			display: grid;
		}
		.error {
			order: 2;
		}
		.area {
			order: 3;
		}
		footer {
			order: 4;
		}
		header {
			flex-wrap: wrap;
			gap: 6px;
			min-height: 0;
			padding: 8px 12px;
		}
		.brand {
			display: flex;
			margin-right: auto;
		}
		.help {
			display: grid;
		}
		.mode {
			font-size: 0;
			gap: 0;
		}
		.brand span {
			display: none;
		}
		.hints {
			display: none;
		}
		.connect h1 {
			font-size: 26px;
		}
	}
</style>
