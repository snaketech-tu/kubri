/**
 * Kubri in more than one language. The English text is the key: `t('Close')` shows "Close" in
 * English and the German from `de.ts` in German. A text without a translation stays English.
 *
 * The language is set once, when the UI starts, from the setting or else the system. Changing it
 * reloads the UI, so texts made when a module loads follow too.
 */
import de from './de';

export type Language = 'en' | 'de';
export type LanguageSetting = Language | 'system';

const KEY = 'kubri.language';
const MESSAGES: Record<Language, Record<string, string>> = { en: {}, de };

/** The languages to pick from, each named in itself. */
export const LANGUAGES: { value: Language; label: string }[] = [
	{ value: 'en', label: 'English' },
	{ value: 'de', label: 'Deutsch' }
];

/** The language of the first of `preferred` that Kubri has, else English. */
export function systemLanguage(preferred: readonly string[]): Language {
	for (const tag of preferred) {
		const base = tag.toLowerCase().split('-')[0];
		if (base === 'en' || base === 'de') return base;
	}
	return 'en';
}

/** The language picked in the settings, or `system`. */
export function languageSetting(): LanguageSetting {
	try {
		const stored = localStorage.getItem(KEY);
		if (stored === 'en' || stored === 'de') return stored;
	} catch {
		// No storage: follow the system.
	}
	return 'system';
}

/** Keeps `setting` and starts the UI again in that language. */
export function setLanguage(setting: LanguageSetting) {
	try {
		if (setting === 'system') localStorage.removeItem(KEY);
		else localStorage.setItem(KEY, setting);
	} catch {
		return;
	}
	location.reload();
}

function startLanguage(): Language {
	const setting = languageSetting();
	if (setting !== 'system') return setting;
	if (typeof navigator === 'undefined') return 'en';
	return systemLanguage(navigator.languages?.length ? navigator.languages : [navigator.language ?? 'en']);
}

/** The language the UI shows. */
export const language: Language = startLanguage();
let messages = MESSAGES[language];

/** For tests: show `lang` from now on. */
export function useLanguage(lang: Language) {
	messages = MESSAGES[lang];
}

/**
 * `text` in the UI's language, with each `{name}` in it replaced by `values.name`. A value is
 * put in as it is, never translated.
 */
export function t(text: string, values?: Record<string, string | number>): string {
	const message = messages[text] ?? text;
	if (!values) return message;
	return message.replace(/\{(\w+)\}/g, (all, name: string) => (name in values ? String(values[name]) : all));
}

/** `one` or `other` by `count`, translated, with `{n}` set to the count: "1 pod", "3 pods". */
export function plural(count: number, one: string, other: string, values: Record<string, string | number> = {}): string {
	return t(count === 1 ? one : other, { n: count, ...values });
}
