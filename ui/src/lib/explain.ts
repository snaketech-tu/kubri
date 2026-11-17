import type { ExplainPrompt, ExplainProvider } from './api';
import { t } from './i18n';
import { coreText } from './i18n/errors';

/** The models Explain suggests for Anthropic, best first. The first is the default. */
export const ANTHROPIC_MODELS = ['claude-opus-5-5', 'claude-sonnet-5-5', 'claude-haiku-5-5'];
/** What Claude Code takes for `--model` besides full names; empty leaves its own choice. */
export const CLAUDE_CODE_MODELS = ['opus', 'sonnet', 'haiku'];

/** Whether the provider is a tool Kubri runs on this computer, signed in on its own. */
export function runsCommand(provider: ExplainProvider): boolean {
	return provider === 'claude' || provider === 'codex';
}

export function providerName(provider: ExplainProvider): string {
	switch (provider) {
		case 'anthropic':
			return 'Anthropic';
		case 'openai':
			return t('an OpenAI-compatible endpoint');
		case 'claude':
			return 'Claude Code';
		case 'codex':
			return 'Codex';
		default:
			return t('nobody');
	}
}

/** Why a prompt cannot be sent yet, or null when it can. */
export function explainBlocker(prompt: Pick<ExplainPrompt, 'provider' | 'model' | 'hasKey' | 'blocked'>): string | null {
	if (prompt.provider === 'none') {
		return t('Explain is off. Pick a provider in the settings: until then nothing is sent anywhere.');
	}
	if (prompt.blocked) return coreText(prompt.blocked);
	// Claude Code and Codex use their own sign-in and, without a model, their own default.
	if (runsCommand(prompt.provider)) return null;
	if (prompt.provider === 'anthropic' && !prompt.hasKey)
		return t('Add your Anthropic API key in the settings to send this.');
	if (!prompt.model) return t('Name the model to ask in the settings to send this.');
	return null;
}

/** The text of the selection inside `within`, or null when nothing there is selected. */
export function selectedText(within: HTMLElement | undefined): string | null {
	const selection = window.getSelection();
	if (!within || !selection || selection.isCollapsed || !selection.rangeCount) return null;
	const range = selection.getRangeAt(0);
	if (!within.contains(range.commonAncestorContainer)) return null;
	const text = selection.toString().trim();
	return text || null;
}
