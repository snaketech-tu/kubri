// What there is to do on the marked rows of a list, and on the cell a right click was on.
import { canDelete, canRestart, canScale, copyLines } from './actions';
import type { ObjectRef, ResourceInfo, Row } from './api';
import { t } from './i18n';
import type { MenuAction } from './menu';
import { isPods } from './pods';
import { app } from './state.svelte';

export interface MarkedRows {
	resource: ResourceInfo;
	/** The marked rows. */
	chosen: Row[];
	/** Namespaces of a context that is not production as a whole can be marked as production. */
	marksProduction: boolean;
	/** Changes are allowed in this context. */
	writable: boolean;
	target: (row: Row) => ObjectRef;
	markAll: () => void;
	unmarkAll: () => void;
	/** Asks before deleting the rows. */
	remove: (rows: Row[]) => void;
}

/** What there is to do on the marked rows together. */
export function markedActions(m: MarkedRows): MenuAction[] {
	const { resource, chosen, target } = m;
	const out: MenuAction[] = [
		{ group: 'Marked', label: t('Copy names'), keys: ['c'], primary: 0, run: () => copyLines(chosen.map((r) => r.name), (n) => t('Copied {n} names', { n })) }
	];
	if (resource.namespaced) {
		out.push({
			group: 'Marked',
			label: t('Copy namespaces'),
			keys: ['n'],
			primary: 0,
			run: () => copyLines([...new Set(chosen.map((r) => r.namespace ?? ''))], (n) => t('Copied {n} namespaces', { n }))
		});
	}
	out.push(
		{ group: 'Marked', label: t('Mark all'), keys: ['ctrl+a'], primary: 0, run: m.markAll },
		{ group: 'Marked', label: t('Unmark all'), keys: ['ctrl+\\'], primary: 0, run: m.unmarkAll }
	);
	if (m.marksProduction) {
		// A setting in Kubri, so also when changes are off. Names a rule marks stay as they are.
		const rules = chosen.map((r) => app.productionRuleOf(r.name, r.labels));
		const unmarked = chosen.filter((_, i) => !rules[i]).map((r) => r.name);
		const named = chosen.filter((_, i) => rules[i]?.by === 'name').map((r) => r.name);
		if (unmarked.length) {
			out.push({ group: 'Change', label: t('Mark {n} as production', { n: unmarked.length }), keys: [], primary: 0, run: () => void app.setProductionNamespace(unmarked, true) });
		}
		if (named.length) {
			out.push({ group: 'Change', label: t('Unmark {n} as production', { n: named.length }), keys: [], primary: 0, run: () => void app.setProductionNamespace(named, false) });
		}
	}
	if (m.writable && canScale(resource) && !isPods(resource)) {
		out.push({
			group: 'Change',
			label: t('Scale {n}…', { n: chosen.length }),
			keys: ['s'],
			primary: 0,
			run: () => (app.dialog = { kind: 'scaleMany', resource, targets: chosen.map(target) })
		});
	}
	if (m.writable && canRestart(resource)) {
		out.push({
			group: 'Change',
			label: t('Restart {n}…', { n: chosen.length }),
			keys: ['r'],
			primary: 0,
			run: () => (app.dialog = { kind: 'restartMany', resource, targets: chosen.map(target) })
		});
	}
	if (m.writable && canDelete(resource)) {
		out.push({
			group: 'Delete',
			label: t('Delete {n}…', { n: chosen.length }),
			keys: ['ctrl+d', 'Delete'],
			danger: true,
			primary: 0,
			run: () => m.remove(chosen)
		});
	}
	return out;
}

/**
 * The row menu with copying the cell right-clicked, or its whole column as the list shows it.
 * They join the Copy group, after what people do most: view, connect, change.
 */
export function withCellActions(actions: MenuAction[], column: string, text: string, columnTexts: () => string[]): MenuAction[] {
	const copy: MenuAction[] = [
		{
			group: 'Copy',
			label: text ? t('Copy “{text}”', { text: text.length > 32 ? `${text.slice(0, 31)}…` : text }) : t('Copy cell'),
			keys: [],
			run: () => copyLines([text])
		},
		{
			group: 'Copy',
			label: t('Copy the {column} column', { column }),
			keys: [],
			run: () => copyLines(columnTexts(), (n) => t('Copied {n} values', { n }))
		}
	];
	const at = actions.findIndex((a) => a.group === 'Copy');
	return at === -1 ? [...actions, ...copy] : [...actions.slice(0, at), ...copy, ...actions.slice(at)];
}
