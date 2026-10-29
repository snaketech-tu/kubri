import type { ExplainPrompt, ExplainProvider } from './api';

/** The models Explain suggests for Anthropic, best first. The first is the default. */
export const ANTHROPIC_MODELS = ['claude-opus-5-5', 'claude-sonnet-5-5', 'claude-haiku-5-5'];

export function providerName(provider: ExplainProvider): string {
	switch (provider) {
		case 'anthropic':
			return 'Anthropic';
		case 'openai':
			return 'an OpenAI-compatible endpoint';
		default:
			return 'nobody';
	}
}

/** Why a prompt cannot be sent yet, or null when it can. */
export function explainBlocker(prompt: Pick<ExplainPrompt, 'provider' | 'model' | 'hasKey'>): string | null {
	if (prompt.provider === 'none') {
		return 'Explain is off. Pick a provider and add your API key in the settings: until then nothing is sent anywhere.';
	}
	if (prompt.provider === 'anthropic' && !prompt.hasKey)
		return 'Add your Anthropic API key in the settings to send this.';
	if (!prompt.model) return 'Name the model to ask in the settings to send this.';
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
