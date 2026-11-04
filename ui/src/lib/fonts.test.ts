import { describe, expect, it } from 'vitest';
import { fontStack } from './fonts';

const BASE = " ui-monospace, 'Kubri Mono', 'Kubri Symbols', monospace";

describe('fontStack', () => {
	it('is Kubri’s own without a setting', () => {
		expect(fontStack('', BASE)).toBe("ui-monospace, 'Kubri Mono', 'Kubri Symbols', monospace");
		expect(fontStack(undefined, BASE)).toBe("ui-monospace, 'Kubri Mono', 'Kubri Symbols', monospace");
	});

	it('quotes the names the user gives and puts them first', () => {
		expect(fontStack('MesloLGS NF, Source Code Pro 2', BASE)).toBe(
			`"MesloLGS NF", "Source Code Pro 2", ${BASE.trim()}`
		);
	});

	it('keeps quoted names and generic families as they are', () => {
		expect(fontStack(`'Fira Code', monospace, "Hack"`, 'monospace')).toBe(`'Fira Code', monospace, "Hack", monospace`);
	});

	it('drops empty entries and stray quotes', () => {
		expect(fontStack(' , My"Font ,', 'monospace')).toBe('"MyFont", monospace');
	});

	it('falls back to monospace without a base', () => {
		expect(fontStack('', '')).toBe('monospace');
	});
});
