import { describe, expect, it } from 'vitest';
import type { BenchInfo } from './api';
import { duration, failureShare, parseHeaders, pathOf, progress, rate, statusTone, ticks } from './bench';

function run(extra: Partial<BenchInfo> = {}): BenchInfo {
	return {
		id: 1,
		forwardId: 1,
		target: { resource: { group: '', version: 'v1', plural: 'pods' }, namespace: 'shop', name: 'web' },
		pod: 'web',
		url: 'http://127.0.0.1:8080/api?x=1',
		method: 'GET',
		concurrency: 2,
		requests: 200,
		durationSecs: null,
		path: '/api?x=1',
		headers: {},
		body: null,
		timeoutSecs: 20,
		started: '',
		state: 'running',
		completed: 50,
		elapsed: 2,
		results: null,
		...extra
	};
}

describe('benchmarks', () => {
	it('formats response times in a readable unit', () => {
		expect(duration(0.00042)).toBe('420µs');
		expect(duration(0.0042)).toBe('4.20ms');
		expect(duration(0.0421)).toBe('42.1ms');
		expect(duration(0.421)).toBe('421ms');
		expect(duration(4.2)).toBe('4.20s');
		expect(duration(null)).toBe('—');
		expect(rate(38.24)).toBe('38.2');
		expect(rate(1204.4)).toBe('1,204');
	});

	it('measures progress by requests or time', () => {
		expect(progress(run())).toBe(0.25);
		expect(progress(run({ requests: null, durationSecs: 10, elapsed: 5 }))).toBe(0.5);
		expect(progress(run({ state: 'done' }))).toBe(1);
	});

	it('reads headers line by line', () => {
		expect(parseHeaders('Host: shop.acme.io\n\nX-Token:  abc:def ')).toEqual({ Host: 'shop.acme.io', 'X-Token': 'abc:def' });
		expect(() => parseHeaders('no colon')).toThrow();
	});

	it('makes round axis ticks', () => {
		expect(ticks(87)).toEqual([0, 50, 100]);
		expect(ticks(40)).toEqual([0, 10, 20, 30, 40]);
		expect(ticks(0)).toEqual([0]);
	});

	it('counts errors and 5xx as failed', () => {
		const results = {
			total: 100,
			duration: 1,
			requestsPerSecond: 100,
			fastest: 0.001,
			slowest: 0.1,
			average: 0.01,
			percentiles: [],
			histogram: [],
			statusCodes: [
				{ code: 200, count: 90 },
				{ code: 404, count: 4 },
				{ code: 503, count: 4 }
			],
			errors: [{ message: 'connection reset', count: 2 }],
			bytes: 0
		};
		expect(failureShare(run({ state: 'done', results }))).toBe(0.06);
		expect([statusTone(204), statusTone(301), statusTone(404), statusTone(502)]).toEqual(['ok', 'muted', 'warn', 'bad']);
		expect(pathOf('http://127.0.0.1:8080/api?x=1')).toBe('/api?x=1');
	});
});
