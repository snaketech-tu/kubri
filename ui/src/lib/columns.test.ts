import { describe, expect, it } from 'vitest';
import { NO_COLUMNS, columnRank, columnWidth, columnsFor, defaultColumns, isShown, kindKey, moved, ordered, overflowing, pathsOf, toggled } from './columns';

const col = (label: string, wideOnly = false) => ({ label, wideOnly });
const labels = (cs: { label: string }[]) => cs.map((c) => c.label);

describe('column settings', () => {
	it('names kinds and paths', () => {
		expect(kindKey({ group: '', kind: 'Pod' })).toBe('/Pod');
		expect(kindKey({ group: 'apps', kind: 'Deployment' })).toBe('apps/Deployment');
		expect(pathsOf(NO_COLUMNS)).toBeNull();
		expect(pathsOf({ ...NO_COLUMNS, custom: [{ name: 'Node', path: '.spec.nodeName' }, { name: 'Img', path: '.spec.x' }] })).toBe(
			'.spec.nodeName\n.spec.x'
		);
	});

	it('takes the defaults from the view until the settings name the kind', () => {
		const view = { columns: [{ name: 'Revision', path: '.status.artifact.revision' }] };
		expect(defaultColumns(undefined)).toBe(NO_COLUMNS);
		expect(defaultColumns({ columns: [] })).toBe(NO_COLUMNS);
		expect(columnsFor('x/Y', undefined, view).custom).toEqual(view.columns);
		const mine = { ...NO_COLUMNS, hidden: ['Age'] };
		expect(columnsFor('x/Y', { 'x/Y': mine }, view)).toBe(mine);
	});

	it('hides, shows wide columns and keeps that apart from the wide view', () => {
		const ip = col('IP', true);
		const ready = col('Ready');
		expect(isShown(ip, NO_COLUMNS, false)).toBe(false);
		expect(isShown(ip, NO_COLUMNS, true)).toBe(true);
		const showIp = toggled(NO_COLUMNS, ip, true);
		expect(isShown(ip, showIp, false)).toBe(true);
		const hideReady = toggled(showIp, ready, false);
		expect(isShown(ready, hideReady, true)).toBe(false);
		expect(isShown(ready, toggled(hideReady, ready, true), false)).toBe(true);
		expect(isShown(ip, toggled(showIp, ip, false), true)).toBe(false);
	});

	it('orders by the settings and keeps unnamed columns beside their neighbors', () => {
		const columns = [col('Name'), col('Ready'), col('Status'), col('Age')];
		expect(labels(ordered(columns, NO_COLUMNS))).toEqual(['Name', 'Ready', 'Status', 'Age']);
		const settings = moved(NO_COLUMNS, labels(columns), 'Age', -2);
		expect(labels(ordered(columns, settings))).toEqual(['Name', 'Age', 'Ready', 'Status']);
		// A column the settings do not know yet follows the one before it.
		const more = [col('Name'), col('Ready'), col('CPU'), col('Status'), col('Age')];
		expect(labels(ordered(more, settings))).toEqual(['Name', 'Age', 'Ready', 'CPU', 'Status']);
		expect(moved(settings, labels(columns), 'Name', -1)).toBe(settings);
	});
});

describe('fitting columns', () => {
	const col = (label: string, rank: number, width: number) => ({ label, rank, width });

	it('leaves out nothing when everything fits', () => {
		expect(overflowing([col('Name', Infinity, 300), col('Age', 55, 60)], 400)).toEqual([]);
	});

	it('drops the lowest rank first, and of equal ranks the rightmost', () => {
		const columns = [col('Name', Infinity, 300), col('Ready', 90, 60), col('A', 50, 80), col('B', 50, 80), col('Age', 55, 60)];
		expect(overflowing(columns, 520)).toEqual(['B']);
		expect(overflowing(columns, 440)).toEqual(['A', 'B']);
		expect(overflowing(columns, 380)).toEqual(['A', 'B', 'Age']);
	});

	it('never drops the name or the state, even when they do not fit', () => {
		expect(overflowing([col('Name', Infinity, 300), col('Status', Infinity, 120), col('Age', 55, 60)], 200)).toEqual(['Age']);
	});

	it('ranks the name and state above readiness, metrics and ages', () => {
		expect(columnRank('Name', 'cell', true)).toBe(Infinity);
		expect(columnRank('Status', 'cell')).toBe(Infinity);
		const order = [columnRank('Ready', 'cell'), columnRank('Namespace', 'namespace'), columnRank('CPU', 'metric'), columnRank('Restarts', 'cell'), columnRank('Age', 'cell'), columnRank('CPU / request', 'metric')];
		expect([...order].sort((a, b) => b - a)).toEqual(order);
	});

	it('measures a column by its longest text or its title', () => {
		expect(columnWidth('Age', ['5m', '12d'])).toBeCloseTo(3 * 6.9 + 14 + 16);
		expect(columnWidth('Name', ['a-very-long-pod-name-1234'], { mono: true })).toBeCloseTo(25 * 7.6 + 16);
		expect(columnWidth('Status', ['ImagePullBackOff'], { icon: true })).toBeCloseTo(16 * 7.1 + 21 + 16);
		expect(columnWidth('Name', ['x'.repeat(200)], { mono: true })).toBe(420 + 16);
	});
});
