import { describe, expect, it } from 'vitest';
import { certificateHealth, certificateStatus } from './certs';
import { relative } from './format';

const now = Date.parse('2026-10-29T12:00:00Z');
const cert = (ready: string, notBefore: string, notAfter: string) => ({
	status: { notBefore, notAfter, conditions: [{ type: 'Ready', status: ready }] }
});

describe('certificateHealth', () => {
	it('judges expiry by lifetime', () => {
		expect(certificateHealth(cert('True', '2026-10-01T12:00:00Z', '2026-12-28T12:00:00Z'), now)).toEqual({
			health: 'ok',
			text: 'Ready, expires in 60 days'
		});
		expect(certificateHealth(cert('True', '2026-08-20T12:00:00Z', '2026-11-18T12:00:00Z'), now).health).toBe('warning');
		expect(certificateHealth(cert('True', '2026-08-05T12:00:00Z', '2026-11-03T12:00:00Z'), now).health).toBe('failed');
		expect(certificateHealth(cert('True', '2026-10-29T08:00:00Z', '2026-10-30T08:00:00Z'), now).health).toBe('ok');
		expect(certificateHealth(cert('True', '2026-09-01T12:00:00Z', '2026-10-29T10:00:00Z'), now).text).toBe('Expired 2 hours ago');
		expect(certificateHealth({}, now)).toEqual({ health: 'warning', text: 'Not issued yet' });
	});

	it('lists what the overview shows', () => {
		const status = certificateStatus({ group: 'cert-manager.io', version: 'v1', plural: 'certificates' }, {
			spec: { secretName: 'web-tls', dnsNames: ['a.example.com'], issuerRef: { kind: 'ClusterIssuer', name: 'le' } },
			status: { notAfter: '2026-12-28T12:00:00Z', conditions: [{ type: 'Ready', status: 'True' }] }
		}, now)!;
		expect(status.facts.map((f) => [f.label, f.value])).toEqual([
			['Expires', '2026-12-28T12:00:00Z'],
			['Issuer', 'ClusterIssuer le'],
			['Secret', 'web-tls'],
			['Names', 'a.example.com']
		]);
		expect(certificateStatus({ group: '', version: 'v1', plural: 'secrets' }, {}, now)).toBeNull();
	});
});

describe('relative', () => {
	it('reads both ways', () => {
		expect(relative('2026-12-28T12:00:00Z', now)).toBe('in 60d');
		expect(relative('2026-10-28T12:00:00Z', now)).toBe('24h ago');
		expect(relative('nonsense', now)).toBe('');
	});
});
