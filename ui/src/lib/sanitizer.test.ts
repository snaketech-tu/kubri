import { describe, expect, it } from 'vitest';
import type { Finding, SanitizerReport, SanitizerSection } from './api';
import { filterFindings, gradeTone, initialExpanded, sanitizerRows, scoreTone, sortedSections, totals } from './sanitizer';

function finding(name: string, levels: Finding['level'][]): Finding {
	const order = { info: 0, warning: 1, error: 2 };
	const level = levels.reduce((a, b) => (order[b] > order[a] ? b : a), 'info' as Finding['level']);
	return {
		namespace: 'shop',
		name,
		level,
		issues: levels.map((l, i) => ({ level: l, code: `c${i}`, message: `${l} on ${name}`, container: i ? 'app' : null }))
	};
}

function section(kind: string, plural: string, findings: Finding[], score: number | null = 80): SanitizerSection {
	return {
		kind,
		resource: { group: '', version: 'v1', plural },
		total: 10,
		ok: 10 - findings.filter((f) => f.level !== 'info').length,
		warning: findings.filter((f) => f.level === 'warning').length,
		error: findings.filter((f) => f.level === 'error').length,
		score,
		findings,
		failed: score === null ? 'forbidden' : null
	};
}

const report: SanitizerReport = {
	score: 72,
	grade: 'C',
	namespace: null,
	sections: [
		section('Pod', 'pods', [finding('cart', ['error', 'info']), finding('web', ['warning']), finding('db', ['info'])]),
		section('ConfigMap', 'configmaps', [finding('stale', ['info'])], 100),
		section('Secret', 'secrets', [], null)
	]
};

describe('sanitizer', () => {
	it('keeps findings and issues at or above a level', () => {
		const pods = report.sections[0].findings;
		expect(filterFindings(pods, 'info', '').map((f) => f.name)).toEqual(['cart', 'web', 'db']);
		const warnings = filterFindings(pods, 'warning', '');
		expect(warnings.map((f) => f.name)).toEqual(['cart', 'web']);
		// The info under cart is left out once only warnings and errors are shown.
		expect(warnings[0].issues.map((i) => i.level)).toEqual(['error']);
		expect(filterFindings(pods, 'error', '').map((f) => f.name)).toEqual(['cart']);
	});

	it('searches names, messages and containers', () => {
		const pods = report.sections[0].findings;
		expect(filterFindings(pods, 'info', 'WEB').map((f) => f.name)).toEqual(['web']);
		expect(filterFindings(pods, 'info', 'app').map((f) => f.name)).toEqual(['cart']);
	});

	it('lists kinds and the findings of open ones', () => {
		const open = initialExpanded(report);
		expect([...open]).toEqual(['/pods']);
		const rows = sanitizerRows(report, open, 'warning', '');
		expect(rows.map((r) => (r.type === 'section' ? r.section.kind : r.finding.name))).toEqual([
			'Pod',
			'cart',
			'web',
			'ConfigMap',
			'Secret'
		]);
		// While searching, kinds without a match drop out.
		const found = sanitizerRows(report, new Set(), 'info', 'stale');
		expect(found.map((r) => r.id)).toEqual(['/configmaps']);
	});

	it('adds up the objects and picks tones', () => {
		expect(totals(report)).toEqual({ objects: 30, warning: 1, error: 1 });
		expect([gradeTone('A'), gradeTone('B'), gradeTone('C'), gradeTone('E'), gradeTone('F')]).toEqual([
			'ok',
			'ok',
			'warn',
			'warn',
			'bad'
		]);
		expect([scoreTone(95), scoreTone(60), scoreTone(10)]).toEqual(['ok', 'warn', 'bad']);
	});
});

describe('sortedSections', () => {
	it('puts errors first, then warnings, then the lowest score', () => {
		const a = section('A', 'a', [finding('x', ['warning'])], 50);
		const b = section('B', 'b', [finding('y', ['error'])], 90);
		const c = section('C', 'c', [], 100);
		const d = section('D', 'd', [finding('z', ['warning']), finding('w', ['warning'])], 70);
		expect(sortedSections([c, a, d, b]).map((s) => s.kind)).toEqual(['B', 'D', 'A', 'C']);
	});
});
