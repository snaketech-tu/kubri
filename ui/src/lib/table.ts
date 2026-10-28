import type { Column, Row } from './api';
import { cellTone } from './status';

export interface SortState {
	/** Column index, or -1 for the default order (namespace, then name). */
	column: number;
	descending: boolean;
}

const NUMBER = /^-?\d+(\.\d+)?$/;
const RESTARTS = /^(\d+)( \(.*\))?$/;
/** How kubectl writes durations, such as `45s`, `5m52s` or `3y120d`. */
const DURATION = /^(?:(\d+)y)?(?:(\d+)d)?(?:(\d+)h)?(?:(\d+)m)?(?:(\d+)s)?$/;
const QUANTITY = /^(\d+(?:\.\d+)?)(m|k|M|G|T|P|E|Ki|Mi|Gi|Ti|Pi|Ei)$/;
const FACTORS: Record<string, number> = {
	m: 1e-3,
	k: 1e3,
	M: 1e6,
	G: 1e9,
	T: 1e12,
	P: 1e15,
	E: 1e18,
	Ki: 2 ** 10,
	Mi: 2 ** 20,
	Gi: 2 ** 30,
	Ti: 2 ** 40,
	Pi: 2 ** 50,
	Ei: 2 ** 60
};
/** Columns whose `100m` means millicores, not minutes. */
const AMOUNT_COLUMN = /cpu|mem|request|limit|capacity|storage|size/i;

/** Durations in seconds and quantities as plain numbers, so `2m` sorts after `45s`. */
function measure(column: Column | undefined, value: string): number | null {
	const quantity = QUANTITY.exec(value);
	if (quantity && (quantity[2] !== 'm' || AMOUNT_COLUMN.test(column?.name ?? ''))) {
		return Number(quantity[1]) * FACTORS[quantity[2]];
	}
	const duration = value ? DURATION.exec(value) : null;
	if (duration) {
		const [years, days, hours, minutes, seconds] = duration.slice(1).map((n) => Number(n ?? 0));
		return (((years * 365 + days) * 24 + hours) * 60 + minutes) * 60 + seconds;
	}
	return null;
}

function sortValue(column: Column | undefined, row: Row, index: number): string | number {
	if (column && isAgeColumn(column)) {
		// Newest first should read as "smallest age", so sort by negative creation time.
		return row.created ? -Date.parse(row.created) : 0;
	}
	const value = row.cells[index];
	if (typeof value === 'number') return value;
	if (typeof value !== 'string') return '';
	if (NUMBER.test(value)) return Number(value);
	const restarts = RESTARTS.exec(value);
	if (restarts && column?.name === 'Restarts') return Number(restarts[1]);
	if (/^\d+\/\d+$/.test(value)) {
		const [a, b] = value.split('/').map(Number);
		return b ? a / b : 0;
	}
	return measure(column, value) ?? value.toLowerCase();
}

export function isAgeColumn(column: Column): boolean {
	return column.name === 'Age' || (column.name === 'Created At' && column.format === 'date');
}

export function sortRows(rows: Row[], columns: Column[], sort: SortState): Row[] {
	const sorted = [...rows];
	if (sort.column < 0) {
		sorted.sort(
			(a, b) => (a.namespace ?? '').localeCompare(b.namespace ?? '') || a.name.localeCompare(b.name)
		);
	} else {
		const column = columns[sort.column];
		sorted.sort((a, b) => {
			const x = sortValue(column, a, sort.column);
			const y = sortValue(column, b, sort.column);
			// Numbers before text such as `<none>`, so the order stays the same either way round.
			const order =
				typeof x === 'number'
					? typeof y === 'number'
						? x - y
						: -1
					: typeof y === 'number'
						? 1
						: x.localeCompare(y);
			return order || a.name.localeCompare(b.name);
		});
	}
	return sort.descending ? sorted.reverse() : sorted;
}

/**
 * Keeps the rows that match `filter`, as k9s filters: a case-insensitive regular expression on
 * the name, namespace and cells; `!` in front keeps the rows that do not match; `-f` matches
 * names fuzzily; `-l app=web` filters by label.
 */
export function filterRows(rows: Row[], filter: string): Row[] {
	const f = filter.trim();
	if (!f) return rows;
	const label = /^-l\s+(.+)$/.exec(f);
	if (label) {
		const selectors = label[1].split(',').map((s) => s.trim().split('='));
		return rows.filter((r) => selectors.every(([k, v]) => (v === undefined ? k in r.labels : r.labels[k] === v)));
	}
	const fuzzy = /^-f\s*(.+)$/.exec(f);
	if (fuzzy) {
		const needle = fuzzy[1].trim().toLowerCase();
		return rows.filter((r) => inOrder(needle, r.name.toLowerCase()));
	}
	const inverse = f.startsWith('!');
	const matches = textMatcher(inverse ? f.slice(1).trim() : f);
	return rows.filter((r) => {
		const hit =
			matches(r.name) || matches(r.namespace ?? '') || r.cells.some((c) => typeof c === 'string' && matches(c));
		return hit !== inverse;
	});
}

/** A case-insensitive regular expression, or the text itself while it is not a valid one. */
function textMatcher(text: string): (value: string) => boolean {
	try {
		const re = new RegExp(text, 'i');
		return (value) => re.test(value);
	} catch {
		const needle = text.toLowerCase();
		return (value) => value.toLowerCase().includes(needle);
	}
}

/** Whether the characters of `needle` appear in `text` in the same order. */
function inOrder(needle: string, text: string): boolean {
	let at = 0;
	for (const c of needle) {
		at = text.indexOf(c, at) + 1;
		if (at === 0) return false;
	}
	return true;
}

/** A row that needs a look, for k9s Ctrl+Z: a bad status, or a warning on an unfinished one. */
export function isFaulty(columns: Column[], row: Row): boolean {
	const cells = columns.map((c, i) => ({ name: c.name, value: row.cells[i], tone: cellTone(c.name, row.cells[i]) }));
	const done = cells.some(
		(c) => /^(status|phase)$/i.test(c.name) && /^(Completed|Complete|Succeeded)$/.test(String(c.value))
	);
	return cells.some((c) => c.tone === 'bad' || (c.tone === 'warn' && !done));
}

/** The table as CSV, the way k9s saves it with Ctrl+S. */
export function toCsv(header: string[], lines: string[][]): string {
	const field = (value: string) => (/[",\n\r]/.test(value) ? `"${value.replaceAll('"', '""')}"` : value);
	return [header, ...lines].map((line) => line.map(field).join(',')).join('\n') + '\n';
}
