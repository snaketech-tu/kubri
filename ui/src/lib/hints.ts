// The keys the footer offers for the view on screen, as k9s shows them in its header.
import { canDelete, canEdit, canRestart, canScale } from './actions';
import { t } from './i18n';
import { parseShortcut, pluginApplies, shortcutLabel } from './k9s';
import { canPortForward, hasImages, hasLogs, hasUsers, isCronJobs, isNamespaces, isNodes, isPods, isReplicaSets, isSecrets } from './pods';
import type { AppState } from './state.svelte';

/** A key as the footer writes it, and what it does. */
export type Hint = [string, string];

/** What the hints depend on. */
export type HintState = Pick<
	AppState,
	'connecting' | 'cluster' | 'contexts' | 'details' | 'page' | 'selected' | 'listRows' | 'scope' | 'readOnly' | 'k9s' | 'pluginView'
> & { layout: { maximized: boolean } };

/** The keys of the view, most used first. */
export function viewHints(app: HintState): Hint[] {
	// The view waits under the connecting card; only cancelling works.
	if (app.connecting && app.cluster && app.contexts.some((c) => c.name === app.connecting)) return [['esc', t('cancel')]];
	const panel: Hint[] = app.details ? [['f', app.layout.maximized ? t('restore') : t('maximize')], ['esc', t('close')]] : [];
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
		];
	}
	// Typing goes to the shell. The panel's keys work there as buttons, and after Esc twice.
	if (app.details?.tab === 'shell') {
		return [['esc esc', t('leave terminal')], ['ctrl+d', t('exit shell')], ...panel, ['?', t('help')]];
	}
	if (app.page === 'portforwards') {
		return [
			[':', t('command')],
			...(app.cluster && !app.cluster.snapshot ? [['⇧f', t('new forward')] as Hint] : []),
			['c', t('copy url')],
			['o', t('open')],
			['s', t('favorite')],
			['b', t('benchmark')],
			['^d', t('stop')],
			['?', t('help')]
		];
	}
	if (app.page === 'pulses') {
		return [[':', t('command')], ['←→', t('move')], ['enter', t('open list')], ...panel, ['?', t('help')]];
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
		];
	}
	if (app.page === 'kinds') {
		return [[':', t('command')], ['/', t('find')], ['j/k', t('move')], ['enter', t('open')], ['?', t('help')]];
	}
	if (app.page === 'rbac') {
		return [[':', t('command')], ['/', t('filter')], ['j/k', t('move')], ...panel, ['?', t('help')]];
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
		];
	}
	if (app.page === 'create') {
		return [[':', t('command')], ['^s', t('review')], ['esc', t('back to editing')], ['?', t('help')]];
	}
	if (app.page === 'config') {
		return [[':', t('command')], ['⇧r', t('reload')], ['?', t('help')]];
	}
	if (app.page === 'benchmarks') {
		return [[':', t('command')], ['j/k', t('move')], ['b', t('run again')], ['^d', t('stop/remove')], ['?', t('help')]];
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
		];
	}
	// Pages without list keys of their own; the list's keys would do nothing there.
	if (app.page !== 'resources') return [[':', t('command')], ...panel, ['?', t('help')]];
	const r = app.selected;
	const h: Hint[] = [
		[':', t('command')],
		['/', t('filter')],
		['enter', t('open')],
		['d', t('describe')],
		['y', t('yaml')]
	];
	if (!r) return h;
	// An empty list has no row for the keys to act on.
	if (app.listRows === 0) {
		const empty: Hint[] = [[':', t('command')], ['/', t('filter')]];
		if (app.scope) empty.push(['esc', t('back')]);
		if (app.details) empty.push(['f', app.layout.maximized ? t('restore') : t('maximize')]);
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
	if (app.details) h.push(['f', app.layout.maximized ? t('restore') : t('maximize')]);
	h.push(['?', t('help')]);
	return h;
}

/** The view's plugins, as k9s lists them in its menu; a key the view uses stays the view's. */
function pluginHints(app: HintState, taken: Set<string>): Hint[] {
	return (app.k9s?.plugins ?? [])
		.filter((p) => pluginApplies(p, app.pluginView))
		.flatMap((p): Hint[] => {
			const shortcut = parseShortcut(p.shortCut);
			if (!shortcut) return [];
			const key = shortcutLabel(shortcut);
			return p.override || !taken.has(key) ? [[key, p.name]] : [];
		})
		.slice(0, 4);
}

// The footer shows the keys that matter most; `?` lists them all. Esc and help always stay.
const MAX_HINTS = 8;

/** The keys the footer shows for the view, with the view's plugins before help, which stays last. */
export function footerHints(app: HintState): Hint[] {
	const all = viewHints(app);
	const keep = (hint: Hint) => hint[0] === 'esc' || hint[0] === '?';
	let room = MAX_HINTS - all.filter(keep).length;
	const shown = all.length <= MAX_HINTS ? all : all.filter((hint) => keep(hint) || room-- > 0);
	const plugins = pluginHints(app, new Set(all.map(([key]) => key)));
	return [...shown.slice(0, -1), ...plugins, ...shown.slice(-1)];
}
