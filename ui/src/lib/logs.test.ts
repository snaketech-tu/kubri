import { describe, expect, it } from 'vitest';
import {
	atLevel,
	defaultColumns,
	fieldNames,
	foldRows,
	lineMatcher,
	parseQuery,
	pretty,
	rate,
	rowText,
	type ViewLine
} from './logs';

let id = 0;
const line = (text: string, more: Partial<ViewLine> = {}): ViewLine => ({
	id: id++,
	source: 0,
	text,
	kind: 'line',
	...more
});

const sources = [
	{ pod: 'web-1', container: 'app' },
	{ pod: 'web-2', container: 'app' }
];

describe('foldRows', () => {
	it('counts repeats and folds continuation lines under their first line', () => {
		const lines = [
			line('ping'),
			line('ping'),
			line('ping'),
			line('Exception in thread main', { level: 'error' }),
			line('    at a.b(C.java:1)', { continued: true }),
			line('    at a.c(C.java:2)', { continued: true }),
			line('ping')
		];
		const rows = foldRows(lines, true);
		expect(rows.map((r) => [r.line.text, r.count, r.more.length])).toEqual([
			['ping', 3, 0],
			['Exception in thread main', 1, 2],
			['ping', 1, 0]
		]);
		expect(foldRows(lines, false)).toHaveLength(7);
	});

	it('keeps sources apart: a frame of one pod does not join a line of another', () => {
		const rows = foldRows(
			[line('boom', { source: 0 }), line('other pod', { source: 1 }), line('  at x', { source: 0, continued: true })],
			true
		);
		expect(rows.map((r) => r.more.length)).toEqual([1, 0]);
		// The same text from two pods is two rows.
		expect(foldRows([line('hi', { source: 0 }), line('hi', { source: 1 })], true)).toHaveLength(2);
	});

	it('does not fold across marks and markers', () => {
		const rows = foldRows([line('a'), line('', { kind: 'mark', source: null }), line('a')], true);
		expect(rows).toHaveLength(3);
	});
});

describe('queries', () => {
	it('reads field terms only when every word is one', () => {
		expect(parseQuery('level=error status>=500')).toEqual({
			text: '',
			fields: [
				{ key: 'level', op: '=', value: 'error' },
				{ key: 'status', op: '>=', value: '500' }
			]
		});
		expect(parseQuery('a=1 and more')).toEqual({ text: 'a=1 and more', fields: [] });
		expect(parseQuery('path~"/api"').fields[0]).toEqual({ key: 'path', op: '~', value: '/api' });
		// Still being typed.
		expect(parseQuery('status>=').fields).toEqual([]);
	});

	it('matches fields, numbers and levels', () => {
		const match = (q: string, l: ViewLine) => lineMatcher(parseQuery(q), sources)(l);
		const json = line('{}', { fields: { status: 503, path: '/api/cart', user: { id: 'u1' } }, level: 'warn' });
		expect(match('status>=500', json)).toBe(true);
		expect(match('status<500', json)).toBe(false);
		expect(match('path~/api', json)).toBe(true);
		expect(match('user.id=u1', json)).toBe(true);
		expect(match('missing!=x', json)).toBe(true);
		expect(match('missing=x', json)).toBe(false);
		// Levels work on every line, in order.
		expect(match('level=warn', json)).toBe(true);
		expect(match('level>=warn', line('x', { level: 'error' }))).toBe(true);
		expect(match('level>=warn', line('x', { level: 'info' }))).toBe(false);
		expect(match('level>=warning', line('x'))).toBe(false);
		expect(match('pod=web-2', line('x', { source: 1 }))).toBe(true);
		// Text and /regex/ as before.
		expect(match('timeout', line('read TIMEOUT'))).toBe(true);
		expect(match('/^read/', line('read x'))).toBe(true);
	});

	it('keeps lines at a minimum level', () => {
		expect(atLevel(line('x', { level: 'warn' }), 'warn')).toBe(true);
		expect(atLevel(line('x', { level: 'info' }), 'warn')).toBe(false);
		expect(atLevel(line('x'), 'warn')).toBe(false);
		expect(atLevel(line('x'), '')).toBe(true);
		expect(atLevel(line('pod joined', { kind: 'marker' }), 'error')).toBe(true);
	});
});

describe('structured lines', () => {
	it('starts a table with the message and the most frequent fields', () => {
		const lines = [
			line('', { fields: { ts: 1, level: 'info', msg: 'a', path: '/', status: 200 } }),
			line('', { fields: { ts: 2, level: 'info', msg: 'b', path: '/x', took: 3 } })
		];
		const names = fieldNames(lines);
		expect(names.slice(0, 4)).toEqual(['level', 'msg', 'path', 'ts']);
		expect(defaultColumns(names)).toEqual(['msg', 'path', 'status', 'took']);
	});

	it('pretty-prints JSON lines only', () => {
		expect(pretty(line('{"a":1}', { fields: { a: 1 } }))).toBe('{\n  "a": 1\n}');
		expect(pretty(line('a=1 b=2', { fields: { a: '1', b: '2' } }))).toBe('a=1 b=2');
	});
});

describe('rowText', () => {
	it('copies what is shown', () => {
		const head = line('boom');
		const rows = foldRows(
			[
				line('hi'),
				line('hi'),
				head,
				line('  at x', { continued: true }),
				line('Pod web-2 joined', { kind: 'marker', source: null })
			],
			true
		);
		const label = () => '';
		expect(rowText(rows, { label, time: false, open: new Set() })).toEqual([
			'hi  (× 2)',
			'boom',
			'    … 1 more line',
			'----- Pod web-2 joined -----'
		]);
		expect(rowText(rows, { label, time: false, open: new Set([head.id]) })[2]).toBe('  at x');
	});
});

describe('rate', () => {
	it('counts lines and errors per slot and remembers the first row', () => {
		const t = (s: number) => new Date(Date.UTC(2026, 9, 29, 10, 0, s)).toISOString();
		const rows = foldRows(
			[
				line('a', { time: t(0) }),
				line('b', { time: t(5), level: 'error', source: 1 }),
				line('c', { time: t(10) }),
				line('Pod joined', { kind: 'marker', source: null, time: t(10) })
			],
			true
		);
		const buckets = rate(rows, 2)!;
		expect(buckets.map((b) => [b.lines, b.errors, b.first])).toEqual([
			[1, 0, 0],
			[2, 1, 1]
		]);
		expect(buckets[1].markers).toEqual(['Pod joined']);
		expect(rate(rows.slice(0, 1))).toBeNull();
		const first = Date.parse(rows[0].line.time!);
		const marked = rate(rows, 2, [{ at: first, text: 'Deployment web: Image' }, { at: first - 60_000, text: 'before' }])!;
		expect(marked[0].markers).toContain('Deployment web: Image');
		expect(marked.flatMap((b) => b.markers)).not.toContain('before');
	});
});
