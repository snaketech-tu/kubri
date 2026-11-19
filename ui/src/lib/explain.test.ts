import { describe, expect, it } from 'vitest';
import { explainBlocker } from './explain';

describe('explainBlocker', () => {
	it('sends only once a provider, a model and, for Anthropic, a key are there', () => {
		const none = { model: null, hasKey: false, blocked: null };
		expect(explainBlocker({ ...none, provider: 'none' })).toMatch(/off/);
		expect(explainBlocker({ ...none, provider: 'anthropic', model: 'claude-opus-5-5' })).toMatch(/API key/);
		expect(explainBlocker({ ...none, provider: 'anthropic', model: 'claude-opus-5-5', hasKey: true })).toBeNull();
		// Local models often need no key, but always a model name.
		expect(explainBlocker({ ...none, provider: 'openai' })).toMatch(/model/);
		expect(explainBlocker({ ...none, provider: 'openai', model: 'llama3.2' })).toBeNull();
		// OpenRouter has a default model, but takes nothing without a key.
		expect(explainBlocker({ ...none, provider: 'openrouter', model: 'openrouter/auto' })).toMatch(/OpenRouter API key/);
		expect(explainBlocker({ ...none, provider: 'openrouter', model: 'openrouter/auto', hasKey: true })).toBeNull();
	});

	it('sends with Claude Code and Codex without a key or model, unless the core says why not', () => {
		const none = { model: null, hasKey: false, blocked: null };
		expect(explainBlocker({ ...none, provider: 'claude' })).toBeNull();
		expect(explainBlocker({ ...none, provider: 'codex', model: 'gpt-6' })).toBeNull();
		const missing = 'Kubri finds no `codex` command. Install Codex, sign in with `codex login`, then try again.';
		expect(explainBlocker({ ...none, provider: 'codex', blocked: missing })).toBe(missing);
	});
});
