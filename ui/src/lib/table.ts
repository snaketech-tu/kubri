import type { Column, Row } from './api';

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
			const order =
				typeof x === 'number' && typeof y === 'number' ? x - y : String(x).localeCompare(String(y));
			return order || a.name.localeCompare(b.name);
		});
	}
	return sort.descending ? sorted.reverse() : sorted;
}

/** Keeps rows whose name, namespace or labels contain `filter`. `-l key=value` filters by label. */
export function filterRows(rows: Row[], filter: string): Row[] {
	const f = filter.trim();
	if (!f) return rows;
	const label = /^-l\s+(.+)$/.exec(f);
	if (label) {
		const selectors = label[1].split(',').map((s) => s.trim().split('='));
		return rows.filter((r) => selectors.every(([k, v]) => (v === undefined ? k in r.labels : r.labels[k] === v)));
	}
	const needle = f.toLowerCase();
	return rows.filter(
		(r) =>
			r.name.toLowerCase().includes(needle) ||
			(r.namespace ?? '').toLowerCase().includes(needle) ||
			r.cells.some((c) => typeof c === 'string' && c.toLowerCase().includes(needle))
	);
}
