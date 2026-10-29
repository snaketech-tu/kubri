import type { Column, Row } from './api/types';

/** Only successful terminal states belong under "finished pods". */
export function completedPods(columns: Column[], rows: Row[]): Row[] {
	const status = columns.findIndex((c) => /^(status|phase)$/i.test(c.name));
	return status < 0 ? [] : rows.filter((r) => /^(Completed|Succeeded)$/.test(String(r.cells[status])));
}
