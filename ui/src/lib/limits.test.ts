import { describe, expect, it } from 'vitest';
import type { Usage } from './api';
import { limitShare, limitSuffix, limitsHint } from './limits';

const MI = 1024 * 1024;
const pod = (more: Partial<Usage> = {}): Usage => ({ namespace: 'shop', name: 'web', cpu: 120, memory: 300 * MI, ...more });

describe('limits in the pod list', () => {
	it('shows the limit after the use, and how full it is', () => {
		const usage = pod({ requests: { cpu: 100, memory: 256 * MI }, limits: { cpu: 500, memory: 512 * MI } });
		expect(limitSuffix(usage, 'cpu')).toBe('/ 500m');
		expect(limitSuffix(usage, 'memory')).toBe('/ 512Mi');
		expect(limitShare(usage, 'cpu')).toBeCloseTo(0.24);
		expect(limitShare(pod({ cpu: 900, limits: { cpu: 500 } }), 'cpu')).toBe(1);
	});

	it('shows nothing without a limit or a reading', () => {
		expect(limitSuffix(pod(), 'cpu')).toBeNull();
		expect(limitShare(pod({ limits: { memory: 512 * MI } }), 'cpu')).toBeNull();
		expect(limitSuffix(undefined, 'cpu')).toBeNull();
		expect(limitsHint(undefined, 'cpu')).toBeNull();
	});

	it('names request and limit in the tooltip', () => {
		expect(limitsHint(pod({ requests: { cpu: 100 }, limits: { cpu: 500 } }), 'cpu')).toBe('Uses 120m, requests 100m, may use at most 500m');
		expect(limitsHint(pod(), 'memory')).toBe('Uses 300Mi, requests nothing. Not every container has a limit, so there is no maximum.');
	});
});

describe('room on a node in the node list', () => {
	const node = (more: Partial<Usage> = {}): Usage => ({ namespace: null, name: 'vm', cpu: 292, memory: 5.5 * 1024 * MI, ...more });

	it('shows what the node can give to pods after its use, and how full it is', () => {
		const usage = node({ requests: { cpu: 400, memory: 544 * MI }, allocatable: { cpu: 4000, memory: 16 * 1024 * MI } });
		expect(limitSuffix(usage, 'cpu')).toBe('/ 4000m');
		expect(limitSuffix(usage, 'memory')).toBe('/ 16Gi');
		expect(limitShare(usage, 'cpu')).toBeCloseTo(0.073);
		expect(limitsHint(usage, 'cpu')).toBe('The node uses 292m of the 4000m it can give to pods; its pods request 400m');
	});

	it('shows nothing when the node does not say what it can give', () => {
		expect(limitSuffix(node(), 'memory')).toBeNull();
		expect(limitShare(node(), 'cpu')).toBeNull();
	});
});
