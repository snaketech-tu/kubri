import { describe, expect, it } from 'vitest';
import { lastEnd } from './forwards';

const now = Date.parse('2026-11-05T10:00:00Z');

describe('how a forward connection ended', () => {
	it('says who closed it and when', () => {
		expect(lastEnd({ lastEnd: { at: '2026-11-05T09:58:00Z', by: 'pod' } }, now)).toBe('Last connection closed by the pod, 2m ago');
		expect(lastEnd({ lastEnd: { at: '2026-11-05T09:59:30Z', by: 'client' } }, now)).toBe('Last connection closed by the client, 30s ago');
	});

	it('leaves errors to the error line, and says nothing before the first end', () => {
		expect(lastEnd({ lastEnd: { at: '2026-11-05T09:59:30Z', by: 'error' } }, now)).toBeNull();
		expect(lastEnd({ lastEnd: null }, now)).toBeNull();
	});
});
