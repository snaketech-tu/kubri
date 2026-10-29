import { describe, expect, it } from 'vitest';
import { completedPods } from './cleanup';
import type { Column, Row } from './api/types';

describe('completed-pod cleanup', () => {
	const columns = [{ name: 'Status' }] as Column[];
	const statuses = [
		'Completed',
		'Succeeded',
		'Pending',
		'CrashLoopBackOff',
		'ImagePullBackOff',
		'Running',
		'Failed',
		'OOMKilled'
	];
	const rows = statuses.map((s) => ({ name: s, cells: [s] })) as Row[];
	it('only selects successful terminal states, never stuck or failed pods', () => {
		expect(completedPods(columns, rows).map((r) => r.name)).toEqual(['Completed', 'Succeeded']);
	});
	it('does not guess when the status column is absent', () => {
		expect(completedPods([], rows)).toEqual([]);
	});
});
