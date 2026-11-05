import { describe, expect, it } from 'vitest';
import { connectHint, connectSummary, noAnswer } from './connect';

describe('connectHint', () => {
	it('says what to try for common failures', () => {
		expect(connectHint('ServiceError: client error (Connect): tcp connect error: Connection refused (os error 111)')).toContain(
			'does not answer'
		);
		expect(connectHint('ApiError: Unauthorized: Unauthorized (ErrorResponse { code: 401 })')).toContain('credentials');
		expect(connectHint('SslError: invalid peer certificate: UnknownIssuer')).toContain('certificate');
		expect(connectHint('ApiError: forbidden: User cannot list resource (403)')).toContain('access');
		expect(connectHint('something else')).toBeNull();
	});
});

describe('connectSummary', () => {
	it('names the kind of problem in a few words', () => {
		expect(connectSummary('timed out while connecting to the cluster')).toBe('Unreachable');
		expect(connectSummary('ServiceError: client error (Connect): tcp connect error: Connection refused (os error 111)')).toBe('Unreachable');
		expect(connectSummary('ApiError: Unauthorized (ErrorResponse { code: 401 })')).toBe('Sign-in failed');
		expect(connectSummary('SslError: invalid peer certificate: UnknownIssuer')).toBe('Certificate not trusted');
		expect(connectSummary('something else')).toBe('Could not connect');
		expect(noAnswer('timed out while connecting to the cluster')).toBe(true);
		expect(noAnswer('ApiError: forbidden (403)')).toBe(false);
	});
});
