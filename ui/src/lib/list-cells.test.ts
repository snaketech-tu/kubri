import { describe, expect, it } from 'vitest';
import type { Column, ResourceInfo, Row, Usage } from './api';
import { cellText, fitColumns, metricColumns, strains, tone, type CellSource, type DisplayColumn } from './list-cells';

const pods: ResourceInfo = {
	group: '',
	version: 'v1',
	plural: 'pods',
	kind: 'Pod',
	singular: 'pod',
	shortNames: ['po'],
	namespaced: true,
	verbs: ['list', 'delete'],
	subresources: ['log']
};
const columns: Column[] = ['Name', 'Ready', 'Status', 'Age'].map((name, i) => ({
	name,
	type: 'string',
	format: i === 0 ? 'name' : '',
	description: '',
	priority: 0
}));
const now = Date.parse('2026-11-10T12:00:00Z');

function source(usage: Partial<Usage> | null = null, extra: Partial<CellSource> = {}): CellSource {
	return {
		resource: pods,
		columns,
		metricsKind: 'pods',
		usage: () => (usage ? ({ namespace: 'shop', name: 'api', cpu: 0, memory: 0, ...usage } as Usage) : undefined),
		trail: () => undefined,
		now,
		taken: null,
		productionRuleOf: () => null,
		thresholds: null,
		...extra
	};
}
function row(status: string, ready = '0/1'): Row {
	return { key: 'shop/api', name: 'api', namespace: 'shop', created: '2026-11-10T11:40:00Z', labels: {}, owners: [], cells: ['api', ready, status, '20m'] };
}
const cell = (label: string, index: number): DisplayColumn => ({ kind: 'cell', label, index, age: label === 'Age', name: index === 0, custom: false, wideOnly: false });
const metric = (label: string): DisplayColumn => {
	const m = metricColumns('pods').find((c) => c.label === label)!;
	return { kind: 'metric', ...m, wideOnly: m.wide };
};

describe('cellText', () => {
	it('counts ages from the snapshot when one is shown', () => {
		expect(cellText(source(), row('Running'), cell('Age', 3))).toBe('20m');
		expect(cellText(source(null, { taken: Date.parse('2026-11-10T11:50:00Z') }), row('Running'), cell('Age', 3))).toBe('10m');
	});

	it('shows no readiness for a finished pod, but keeps it in the CSV', () => {
		expect(cellText(source(), row('Completed'), cell('Ready', 1))).toBe('—');
		expect(cellText(source(), row('Completed'), cell('Ready', 1), true)).toBe('0/1');
	});

	it('writes amounts plainly, and as kubectl does for the CSV', () => {
		const used = source({ cpu: 250, memory: 512 * 1024 * 1024 });
		expect(cellText(used, row('Running'), metric('Mem'), true)).toBe('512Mi');
		expect(cellText(source(), row('Running'), metric('CPU'))).toBe('—');
	});
});

describe('tone', () => {
	it('colors a pod readiness by its status', () => {
		expect(tone(source(), row('CrashLoopBackOff'), cell('Ready', 1), '0/1')).toBe('bad');
		expect(tone(source(), row('Pending'), cell('Ready', 1), '0/1')).toBe('warn');
		expect(tone(source(), row('Running', '1/1'), cell('Ready', 1), '1/1')).toBe('ok');
	});

	it('colors usage near its limit red, and above its request amber', () => {
		const near = source({ cpu: 950, limits: { cpu: 1000 }, requests: { cpu: 100 } });
		expect(tone(near, row('Running'), metric('CPU'), '')).toBe('bad');
		expect(strains(near, row('Running')).map((s) => s.label)).toEqual(['throttled']);
		const over = source({ cpu: 200, requests: { cpu: 100 } });
		expect(tone(over, row('Running'), metric('CPU'), '')).toBe('warn');
		expect(strains(over, row('Running'))).toEqual([]);
	});
});

describe('fitColumns', () => {
	it('leaves out what matters least, and never the name or status', () => {
		const shown = [cell('Name', 0), cell('Ready', 1), cell('Status', 2), cell('Age', 3)];
		const all = fitColumns(source(), shown, [row('Running')], 1000);
		expect(all.left).toEqual([]);
		const narrow = fitColumns(source(), shown, [row('Running')], 200);
		expect(narrow.left).toContain('Age');
		expect(narrow.drawn.map((c) => c.label)).toEqual(expect.arrayContaining(['Name', 'Status']));
	});
});
