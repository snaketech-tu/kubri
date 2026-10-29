import { describe, expect, it } from 'vitest';
import type { HistoryEntry } from './api';
import { parseChangesArgs, timeline } from './changes';

const MIN = 60_000;
const NOW = new Date(2026, 9, 29, 12, 0).getTime();

function entry(id: number, at: number): HistoryEntry {
	return {
		id,
		context: 'k3s',
		at,
		resource: { group: 'apps', version: 'v1', plural: 'deployments' },
		kind: 'Deployment',
		namespace: 'shop',
		name: `web${id}`,
		uid: `u${id}`,
		change: 'image',
		summary: 'Image',
		unwatched: false,
		hasDiff: true
	};
}

const kinds = (items: ReturnType<typeof timeline>) => items.map((i) => (i.type === 'entry' ? `e${i.entry.id}` : i.type === 'gap' ? `gap` : i.label));

describe('timeline', () => {
	it('puts the times Kubri was not watching between the changes', () => {
		const watched = [
			{ from: NOW - 30 * MIN, to: NOW },
			{ from: NOW - 120 * MIN, to: NOW - 60 * MIN }
		];
		const entries = [entry(3, NOW - 10 * MIN), entry(2, NOW - 70 * MIN), entry(1, NOW - 100 * MIN)];
		const items = timeline(entries, watched, NOW - 180 * MIN, NOW, true, true);
		expect(kinds(items)).toEqual(['Today', 'e3', 'gap', 'e2', 'e1', 'gap']);
		expect(items[2]).toMatchObject({ from: NOW - 60 * MIN, to: NOW - 30 * MIN });
		expect(items[5]).toMatchObject({ from: null, to: NOW - 120 * MIN });
	});

	it('says when Kubri is not watching now, and waits for older pages before the oldest gap', () => {
		const watched = [{ from: NOW - 120 * MIN, to: NOW - 60 * MIN }];
		const items = timeline([entry(1, NOW - 90 * MIN)], watched, null, NOW, false, false);
		expect(kinds(items)).toEqual(['Today', 'gap', 'e1']);
		expect(items[1]).toMatchObject({ from: NOW - 60 * MIN, to: null });
	});

	it('leaves out short gaps, and has a heading per day', () => {
		const watched = [
			{ from: NOW - MIN, to: NOW },
			{ from: NOW - 30 * 3600_000, to: NOW - 2 * MIN }
		];
		const items = timeline([entry(2, NOW - 5 * MIN), entry(1, NOW - 26 * 3600_000)], watched, null, NOW, true, true);
		expect(kinds(items)).toEqual(['Today', 'e2', 'Yesterday', 'e1', 'gap']);
	});
});

describe('parseChangesArgs', () => {
	it('reads a namespace and a range in any order', () => {
		expect(parseChangesArgs(['6h', 'shop'])).toEqual({ namespace: 'shop', hours: 6 });
		expect(parseChangesArgs(['30m'])).toEqual({ hours: 0.5 });
		expect(parseChangesArgs(['2d'])).toEqual({ hours: 48 });
		expect(parseChangesArgs([])).toEqual({});
	});
});
