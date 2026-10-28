import type { Finding, Level, SanitizerReport, SanitizerSection } from './api';

const RANK: Record<Level, number> = { info: 0, warning: 1, error: 2 };

/** The lowest level shown: everything, warnings and errors, or errors only. */
export type LevelFilter = Level;

export function atLeast(level: Level, min: LevelFilter) {
	return RANK[level] >= RANK[min];
}

/** A, B: fine; C to E: needs work; F: bad. The letter is the label, the tone repeats it. */
export function gradeTone(grade: string): 'ok' | 'warn' | 'bad' {
	return grade <= 'B' ? 'ok' : grade === 'F' ? 'bad' : 'warn';
}

export function scoreTone(score: number): 'ok' | 'warn' | 'bad' {
	return score >= 80 ? 'ok' : score >= 50 ? 'warn' : 'bad';
}

/** Findings at or above `min`, with their lower issues left out, matching `text`. */
export function filterFindings(findings: Finding[], min: LevelFilter, text: string): Finding[] {
	const t = text.trim().toLowerCase();
	return findings
		.filter((f) => atLeast(f.level, min))
		.map((f) => ({ ...f, issues: f.issues.filter((i) => atLeast(i.level, min)) }))
		.filter(
			(f) =>
				!t ||
				[f.name, f.namespace ?? '', ...f.issues.flatMap((i) => [i.message, i.code, i.container ?? ''])].some((v) =>
					v.toLowerCase().includes(t)
				)
		);
}

export type SanitizerRow =
	| { type: 'section'; id: string; section: SanitizerSection; shown: number }
	| { type: 'finding'; id: string; section: SanitizerSection; finding: Finding };

export function sectionId(s: SanitizerSection) {
	return `${s.resource.group}/${s.resource.plural}`;
}

export function findingId(s: SanitizerSection, f: Finding) {
	return `${sectionId(s)}/${f.namespace ?? ''}/${f.name}`;
}

/** The rows on screen: every kind, and the findings of the expanded ones. */
export function sanitizerRows(
	report: SanitizerReport,
	expanded: Set<string>,
	min: LevelFilter,
	text: string
): SanitizerRow[] {
	const rows: SanitizerRow[] = [];
	for (const section of report.sections) {
		const findings = filterFindings(section.findings, min, text);
		// While searching, kinds without a match only get in the way.
		if (text.trim() && !findings.length) continue;
		const id = sectionId(section);
		rows.push({ type: 'section', id, section, shown: findings.length });
		if (expanded.has(id)) {
			for (const finding of findings) rows.push({ type: 'finding', id: findingId(section, finding), section, finding });
		}
	}
	return rows;
}

/** Kinds with errors start open, so the worst problems are on screen first. */
export function initialExpanded(report: SanitizerReport): Set<string> {
	return new Set(report.sections.filter((s) => s.error > 0).map(sectionId));
}

export function totals(report: SanitizerReport) {
	return report.sections.reduce(
		(t, s) => ({ objects: t.objects + s.total, warning: t.warning + s.warning, error: t.error + s.error }),
		{ objects: 0, warning: 0, error: 0 }
	);
}
