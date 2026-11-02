import { afterEach, describe, expect, it, vi } from 'vitest';
import type { TableEvent, Row } from './api/types';
const mock = vi.hoisted(() => ({ watchTable: vi.fn(), metrics: vi.fn() }));
vi.mock('./api', () => ({ backend: mock }));
import { LiveMetrics, LiveTable } from './watch.svelte';
const resource = { group: '', version: 'v1', plural: 'pods' };
afterEach(() => {
	vi.useRealTimers();
	vi.resetAllMocks();
});
describe('live resource state', () => {
	it('retains rows on errors, ignores late events after stopping, and clears on scope changes', () => {
		let emit!: (e: TableEvent) => void;
		mock.watchTable.mockImplementation((_r, _n, _s, cb) => {
			emit = cb;
			return vi.fn();
		});
		const table = new LiveTable();
		const stop = table.start(resource, 'shop');
		emit({ type: 'reset', columns: [], rows: [{ key: 'one', name: 'one' } as Row] });
		emit({ type: 'error', message: 'Forbidden' });
		expect(table.hasLoaded).toBe(true);
		expect(table.rows).toHaveLength(1);
		stop();
		emit({ type: 'reset', columns: [], rows: [] });
		expect(table.rows).toHaveLength(1);
		table.start(resource, 'other', null, true);
		expect(table.rows).toHaveLength(0);
		expect(table.hasLoaded).toBe(false);
	});
	it('shows the rows of a list left in a context until its watch answers again', () => {
		let emit!: (e: TableEvent) => void;
		mock.watchTable.mockImplementation((_r, _n, _s, cb) => {
			emit = cb;
			return vi.fn();
		});
		const table = new LiveTable();
		let stop = table.start(resource, 'shop', null, false, 'a');
		emit({ type: 'reset', columns: [], rows: [{ key: 'one', name: 'one' } as Row] });
		stop();
		// Another context's list starts empty.
		stop = table.start(resource, 'shop', null, false, 'b');
		expect(table.rows).toHaveLength(0);
		expect(table.loading).toBe(true);
		stop();
		// Back in the first one, the old rows show at once, marked stale.
		stop = table.start(resource, 'shop', null, false, 'a');
		expect(table.rows.map((r) => r.name)).toEqual(['one']);
		expect([table.stale, table.loading, table.hasLoaded]).toEqual([true, false, false]);
		emit({ type: 'reset', columns: [], rows: [{ key: 'two', name: 'two' } as Row] });
		expect(table.rows.map((r) => r.name)).toEqual(['two']);
		expect(table.stale).toBe(false);
		stop();
		// A failure instead of a list drops the old rows rather than showing them as current.
		table.start(resource, 'shop', null, false, 'a');
		expect(table.stale).toBe(true);
		emit({ type: 'error', message: 'Unauthorized' });
		expect([table.stale, table.rows.length]).toEqual([false, 0]);
	});
	it('keeps a successful empty list distinct from an initial error', () => {
		let emit!: (e: TableEvent) => void;
		mock.watchTable.mockImplementation((_r, _n, _s, cb) => {
			emit = cb;
			return vi.fn();
		});
		const table = new LiveTable();
		table.start(resource, null);
		emit({ type: 'error', message: '403 Forbidden' });
		expect(table.hasLoaded).toBe(false);
		expect(table.loading).toBe(false);
		emit({ type: 'reset', columns: [], rows: [] });
		expect(table.hasLoaded).toBe(true);
		expect(table.error).toBeNull();
	});
	it('retains the last successful metrics and recovers from temporary failures', async () => {
		vi.useFakeTimers();
		mock.metrics
			.mockResolvedValueOnce([{ namespace: 'shop', name: 'pod', cpu: 0, memory: 0 }])
			.mockRejectedValueOnce(new Error('Unavailable'))
			.mockResolvedValueOnce([]);
		const metrics = new LiveMetrics();
		const stop = metrics.start('pods', 'shop');
		await vi.advanceTimersByTimeAsync(1);
		expect(metrics.get('shop', 'pod')?.cpu).toBe(0);
		await vi.advanceTimersByTimeAsync(15000);
		expect(metrics.available).toBe(true);
		expect(metrics.error).toBe('Unavailable');
		expect(metrics.get('shop', 'pod')?.cpu).toBe(0);
		await vi.advanceTimersByTimeAsync(15000);
		expect(metrics.error).toBeNull();
		expect(metrics.get('shop', 'pod')).toBeUndefined();
		stop();
	});
});
