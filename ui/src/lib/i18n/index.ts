/**
 * Kubri in more than one language. The English text is the key: `t('Close')` shows "Close" in
 * English and the German from `de.ts` in German. A text without a translation stays English.
 *
 * The language is set once, when the UI starts, from the setting or else the system. Changing it
 * reloads the UI, so texts made when a module loads follow too.
 */
import de from './de';
import { store, stored } from '../storage';

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
	const value = stored(KEY);
	return value === 'en' || value === 'de' ? value : 'system';
}

/** Keeps `setting` and starts the UI again in that language. */
export function setLanguage(setting: LanguageSetting) {
	if (store(KEY, setting === 'system' ? null : setting)) location.reload();
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

let current: Language = language;

/** For tests: show `lang` from now on. */
export function useLanguage(lang: Language) {
	messages = MESSAGES[lang];
	current = lang;
}

/**
 * A name such as "Pods" or "View" inside a sentence: English writes it small ("3 of 5 pods"),
 * German keeps its capital ("3 von 5 Pods").
 */
export function midSentence(name: string): string {
	return current === 'en' ? name.toLowerCase() : name;
}

/**
 * `text` in the UI's language, with each `{name}` in it replaced by `values.name`. A value is
 * put in as it is, never translated.
 */
export function t(text: string, values?: Record<string, string | number>): string {
	const message = Object.hasOwn(messages, text) ? messages[text] : text;
	if (!values) return message;
	return message.replace(/\{(\w+)\}/g, (all, name: string) => (Object.hasOwn(values, name) ? String(values[name]) : all));
}

/** `one` or `other` by `count`, translated, with `{n}` set to the count: "1 pod", "3 pods". */
export function plural(count: number, one: string, other: string, values: Record<string, string | number> = {}): string {
	return t(count === 1 ? one : other, { n: count, ...values });
}

/** A piece of a text with `{name}`s in it: words as they are, or the name of a value. */
export type Part = { text: string } | { name: string };

/**
 * `text` cut at its `{name}`s, for texts whose values are shown in bold or as code: translate the
 * text with `t()` first, then put each value in where its name is.
 */
export function parts(text: string): Part[] {
	return text
		.split(/(\{\w+\})/)
		.filter(Boolean)
		.map((piece) => {
			const name = /^\{(\w+)\}$/.exec(piece)?.[1];
			return name === undefined ? { text: piece } : { name };
		});
}
