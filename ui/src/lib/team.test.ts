import { describe, expect, it } from 'vitest';
import { describeChange } from './team';

describe('describeChange', () => {
	it('names the action, the object and what was asked for', () => {
		expect(
			describeChange({
				time: '2026-11-05T10:00:00Z',
				action: 'scale',
				target: { resource: 'deployments.apps', namespace: 'shop', name: 'web' },
				detail: { replicas: 3 },
				ok: true
			})
		).toBe('scale deployments.apps shop/web · replicas 3');
	});

	it('leaves out what a change does not have', () => {
		expect(describeChange({ time: '', action: 'drain', target: { resource: 'nodes', name: 'n1' }, ok: false })).toBe(
			'drain nodes n1'
		);
		expect(describeChange({ time: '', action: 'change settings', ok: true })).toBe('change settings');
		expect(
			describeChange({ time: '', action: 'port-forward', detail: { remotePort: 80, localPort: 8080 }, ok: true })
		).toBe('port-forward · remote port 80, local port 8080');
	});
});
