import { describe, expect, it } from 'vitest';
import { pickLine } from './lastlog';

describe('pickLine', () => {
	it('prefers the last line that names a problem', () => {
		expect(pickLine(['starting', 'db connection refused', 'shutting down', ''])).toBe('db connection refused');
	});

	it('falls back to the last line, and to nothing', () => {
		expect(pickLine(['starting', 'ready'])).toBe('ready');
		expect(pickLine(['', '  '])).toBeNull();
	});

	it('cuts long lines', () => {
		expect(pickLine(['error ' + 'x'.repeat(300)])!.length).toBe(158);
	});
});
