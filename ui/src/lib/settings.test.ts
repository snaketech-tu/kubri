import { describe, expect, it } from 'vitest';
import { NO_CONTEXT_SETTINGS, legacyContextSettings } from './state.svelte';

describe('legacyContextSettings', () => {
	const stored: Record<string, string> = {
		'kubri.readonly.prod': 'true',
		'kubri.environment.prod': 'Production',
		'kubri.environment.stage': 'Staging',
		'kubri.environment.kept': 'Prod',
		'kubri.toolsBeside': 'true'
	};
	const read = (key: string) => stored[key] ?? null;

	it('moves read-only choices and labels, guessing production from the label', () => {
		const found = legacyContextSettings(Object.keys(stored), read, {});
		expect(found.prod).toEqual({ ...NO_CONTEXT_SETTINGS, readOnly: true, label: 'Production', production: true });
		expect(found.stage).toEqual({ ...NO_CONTEXT_SETTINGS, label: 'Staging' });
		expect(Object.keys(found)).not.toContain('toolsBeside');
	});

	it('leaves contexts alone that already have settings', () => {
		const found = legacyContextSettings(Object.keys(stored), read, { kept: { ...NO_CONTEXT_SETTINGS, color: '#326ce5' } });
		expect(found.kept).toBeUndefined();
		expect(found.prod).toBeDefined();
	});
});
