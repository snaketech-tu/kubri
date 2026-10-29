import { describe, expect, it } from 'vitest';
import { explainBlocker } from './explain';

describe('explainBlocker', () => {
	it('sends only once a provider, a model and, for Anthropic, a key are there', () => {
		expect(explainBlocker({ provider: 'none', model: null, hasKey: false })).toMatch(/off/);
		expect(explainBlocker({ provider: 'anthropic', model: 'claude-opus-5-5', hasKey: false })).toMatch(/API key/);
		expect(explainBlocker({ provider: 'anthropic', model: 'claude-opus-5-5', hasKey: true })).toBeNull();
		// Local models often need no key, but always a model name.
		expect(explainBlocker({ provider: 'openai', model: null, hasKey: false })).toMatch(/model/);
		expect(explainBlocker({ provider: 'openai', model: 'llama3.2', hasKey: false })).toBeNull();
	});
});
