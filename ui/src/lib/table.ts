import type { Column, Row } from './api';

export interface SortState {
	/** Column index, or -1 for the default order (namespace, then name). */
	column: number;
	descending: boolean;
}

const NUMBER = /^-?\d+(\.\d+)?$/;
const RESTARTS = /^(\d+)( \(.*\))?$/;

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
	return value.toLowerCase();
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
