import { describe, expect, it } from 'vitest';
import { readOnlyFixed, readOnlyReason } from './guard';

const open = { readOnly: false, reason: null, canUnlock: false, unlockedFor: null, fixed: false };

describe('readOnlyReason', () => {
	it('names --read-only, not k9s, when the server was started read-only', () => {
		const guard = { ...open, readOnly: true, fixed: true, reason: 'it was started with --read-only or KUBRI_READ_ONLY' };
		expect(readOnlyReason(guard, false, true)).toBe(
			'Kubri is read-only because it was started with --read-only or KUBRI_READ_ONLY'
		);
	});
	it('keeps a fixed reason even when the context is also set read-only', () => {
		const guard = { ...open, readOnly: true, fixed: true, reason: 'this is a snapshot' };
		expect(readOnlyReason(guard, true, false)).toContain('snapshot');
	});
	it('points to the header switch for the read-only setting', () => {
		const guard = { ...open, readOnly: true, reason: 'this context is set read-only in Kubri' };
		expect(readOnlyReason(guard, true, false)).toContain('switch it off in the header');
	});
	it('says how to unlock a locked context', () => {
		const guard = { ...open, readOnly: true, canUnlock: true, reason: 'locked' };
		expect(readOnlyReason(guard, false, false)).toContain('unlock it');
	});
	it('falls back to k9s before the guard is loaded', () => {
		expect(readOnlyReason(open, false, true)).toContain('k9s configuration');
	});
});

describe('readOnlyFixed', () => {
	it('is fixed for a fixed guard or k9s', () => {
		expect(readOnlyFixed({ ...open, readOnly: true, fixed: true }, false, false)).toBe(true);
		expect(readOnlyFixed(open, true, false)).toBe(true);
	});
	it('is not fixed for the setting or a lock', () => {
		expect(readOnlyFixed({ ...open, readOnly: true }, false, true)).toBe(false);
		expect(readOnlyFixed({ ...open, readOnly: true, canUnlock: true }, false, false)).toBe(false);
	});
});
