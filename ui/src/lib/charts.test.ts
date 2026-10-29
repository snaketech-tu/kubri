import { describe, expect, it } from 'vitest';
import { colorSlots, formatValue, timeTicks, valueAt, valueTicks } from './charts';

describe('charts', () => {
	it('formats values in their unit', () => {
		expect(formatValue('millicores', 250)).toBe('250m');
		expect(formatValue('millicores', 0.42)).toBe('0.42m');
		expect(formatValue('millicores', 5)).toBe('5m');
		expect(formatValue('bytes', 64 * 1024 ** 2)).toBe('64Mi');
		expect(formatValue('bytesPerSecond', 2048)).toBe('2KiB/s');
		expect(formatValue('perSecond', 0.0123)).toBe('0.012/s');
		expect(formatValue('count', 3)).toBe('3');
	});

	it('picks round ticks', () => {
		expect(valueTicks(250, 'millicores')).toEqual([0, 100, 200, 300]);
		expect(valueTicks(0, 'millicores')).toEqual([0, 1]);
		expect(valueTicks(2.4, 'count')).toEqual([0, 1, 2, 3]);
		// Binary steps for bytes: 64Mi, not 50M.
		expect(valueTicks(150 * 1024 ** 2, 'bytes')).toEqual([0, 64, 128, 192].map((m) => m * 1024 ** 2));
	});

	it('puts time ticks on round times', () => {
		const ticks = timeTicks(1000, 1000 + 3600);
		expect(ticks.length).toBeGreaterThanOrEqual(2);
		expect(ticks.every((t) => (t - ticks[0]) % 900 === 0)).toBe(true);
	});

	it('keeps a line its color and grays the rest', () => {
		const slots = colorSlots([
			{ name: 'web-b', points: [] },
			{ name: 'web-a', points: [] },
			{ name: '3 others', points: [] }
		]);
		expect(slots.get('web-a')).toBe(1);
		expect(slots.get('web-b')).toBe(2);
		expect(slots.get('3 others')).toBe(null);
	});

	it('reads the value nearest a time', () => {
		const series = {
			name: 'x',
			points: [
				[100, 1],
				[130, 2]
			] as [number, number][]
		};
		expect(valueAt(series, 128, 30)).toBe(2);
		expect(valueAt(series, 200, 30)).toBe(null);
	});
});
