import { describe, expect, it } from 'vitest';
import { connectHint } from './connect';

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
