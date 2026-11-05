import { afterEach, describe, expect, it } from 'vitest';
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join } from 'node:path';
import de from './de';
import { CHECK_FORMATS, coreText, CORE_TEXTS, ERROR_FORMATS, NOTICE_FORMATS } from './errors';
import { parts, plural, systemLanguage, t, useLanguage } from '.';

const SRC = new URL('../..', import.meta.url).pathname;

/** Every .ts and .svelte file of the UI, without tests and this folder. */
function sources(dir = SRC): string[] {
	return readdirSync(dir).flatMap((name) => {
		const path = join(dir, name);
		if (statSync(path).isDirectory()) return name === 'i18n' ? [] : sources(path);
		return /\.(ts|svelte)$/.test(name) && !name.endsWith('.test.ts') ? [path] : [];
	});
}

const STRING = String.raw`'((?:[^'\\]|\\.)*)'|"((?:[^"\\]|\\.)*)"`;
const unescape = (s: string) => s.replace(/\\(.)/g, '$1');

/** The texts given to t() and plural() in the UI's code. */
function used(): Map<string, string> {
	const texts = new Map<string, string>();
	const call = new RegExp(String.raw`(?<![\w$.])t\(\s*(?:${STRING})`, 'g');
	const many = new RegExp(String.raw`(?<![\w$.])plural\(\s*[^,]+,\s*(?:${STRING})\s*,\s*(?:${STRING})`, 'g');
	for (const file of sources()) {
		const code = readFileSync(file, 'utf8');
		for (const m of code.matchAll(call)) texts.set(unescape(m[1] ?? m[2]), file);
		for (const m of code.matchAll(many)) {
			texts.set(unescape(m[1] ?? m[2]), file);
			texts.set(unescape(m[3] ?? m[4]), file);
		}
	}
	// Texts from the core are translated as they arrive.
	for (const text of [...ERROR_FORMATS, ...CHECK_FORMATS, ...NOTICE_FORMATS, ...CORE_TEXTS]) texts.set(text, 'errors.ts');
	return texts;
}

const placeholders = (s: string) => [...s.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort();

describe('i18n', () => {
	afterEach(() => useLanguage('en'));

	it('follows the system when it speaks German', () => {
		expect(systemLanguage(['de-AT', 'en'])).toBe('de');
		expect(systemLanguage(['fr-FR', 'de'])).toBe('de');
		expect(systemLanguage(['en-GB'])).toBe('en');
		expect(systemLanguage(['fr'])).toBe('en');
		expect(systemLanguage([])).toBe('en');
	});

	it('translates and fills in values', () => {
		expect(t('Overview')).toBe('Overview');
		expect(t('Kubri {version} is out', { version: '1.8.0' })).toBe('Kubri 1.8.0 is out');
		useLanguage('de');
		expect(t('Overview')).toBe('Übersicht');
		expect(t('Kubri {version} is out', { version: '1.8.0' })).toBe('Kubri 1.8.0 ist da');
		expect(t('A text without German')).toBe('A text without German');
		expect(t('{missing} stays', {})).toBe('{missing} stays');
	});

	it('picks one or other by the count', () => {
		expect(plural(1, '{n} pod', '{n} pods')).toBe('1 pod');
		expect(plural(3, '{n} pod', '{n} pods')).toBe('3 pods');
	});

	it('has German for every text of the UI', () => {
		const missing = [...used()].filter(([text]) => !(text in de)).map(([text, file]) => `${file.slice(SRC.length)}: ${text}`);
		expect(missing).toEqual([]);
	});

	it('has no German left over for texts the UI no longer uses', () => {
		const texts = used();
		expect(Object.keys(de).filter((text) => !texts.has(text))).toEqual([]);
	});

	it('keeps the placeholders of each text', () => {
		const wrong = Object.entries(de).filter(([en, text]) => placeholders(en).join() !== placeholders(text).join());
		expect(wrong).toEqual([]);
	});

	it('knows every error message format of the core', () => {
		const rust = readFileSync(new URL('../../../../crates/kubri-core/src/error.rs', import.meta.url), 'utf8');
		const main = rust.slice(0, rust.indexOf('#[cfg(test)]'));
		const formats = [...main.matchAll(/#\[error\("((?:[^"\\]|\\.)*)"/g)]
			.map((m) => m[1].replace(/\\"/g, '"').replace(/\{\d\}/g, '{0}'))
			.filter((f) => f !== '{0}' && f !== '{}');
		const inKubeMessage = [...main.matchAll(/format!\(\s*"((?:[^"\\]|\\.)*)"/g)].map((m) => m[1].replace('({})', '({0})'));
		expect([...formats, ...inKubeMessage].sort()).toEqual([...ERROR_FORMATS].sort());
	});

	it('knows how the core names a check', () => {
		const rust = readFileSync(new URL('../../../../crates/kubri-core/src/access.rs', import.meta.url), 'utf8');
		const formats = [...rust.matchAll(/format!\("(`\{\}` on [^"]*)"/g)].map((m) => {
			let i = 0;
			return m[1].replace(/\{\w*\}/g, () => `{${i++}}`);
		});
		expect(formats).toEqual(CHECK_FORMATS);
	});

	it('knows the notices above a list', () => {
		const rust = readFileSync(new URL('../../../../crates/kubri-core/src/lib.rs', import.meta.url), 'utf8');
		const numbered = (format: string) => {
			let i = 0;
			return format.replace(/\{\w*\}/g, () => `{${i++}}`);
		};
		const notices = [...rust.matchAll(/let notice = format!\(\s*"((?:[^"\\]|\\.)*)"/g)].map((m) => numbered(m[1]));
		expect(notices).toEqual(NOTICE_FORMATS.slice(0, notices.length));
		expect(rust).toContain('format!("{} and {} more"');
		expect(notices.length + 1).toBe(NOTICE_FORMATS.length);
	});

	it('cuts a text at its names, for values in bold', () => {
		expect(parts('Restart {name} now?')).toEqual([{ text: 'Restart ' }, { name: 'name' }, { text: ' now?' }]);
		expect(parts('{n} of {total}')).toEqual([{ name: 'n' }, { text: ' of ' }, { name: 'total' }]);
		expect(parts('No names')).toEqual([{ text: 'No names' }]);
	});

	it('translates errors from the core, values and all', () => {
		useLanguage('de');
		expect(coreText('not connected to a cluster, select a context first')).toBe('nicht mit einem Cluster verbunden, wähle zuerst einen Kontext');
		expect(coreText('context `prod` does not exist in the kubeconfig')).toBe('den Kontext `prod` gibt es in der kubeconfig nicht');
		expect(coreText('Kubri is read-only (this context is set read-only in Kubri), so it does not change the cluster')).toBe(
			'Kubri ist schreibgeschützt (dieser Kontext ist in Kubri schreibgeschützt) und ändert den Cluster daher nicht'
		);
		expect(coreText('not allowed: needs `delete` on `pods` in `shop`')).toBe('nicht erlaubt: braucht `delete` auf `pods` in `shop`');
		expect(coreText('not allowed: needs `patch` on `deployments.apps/scale`')).toBe('nicht erlaubt: braucht `patch` auf `deployments.apps/scale`');
		expect(coreText('pods "web" not found')).toBe('pods "web" not found');
	});

	it('only takes texts written out, so the test above sees them', () => {
		const dynamic = sources().flatMap((file) =>
			[...readFileSync(file, 'utf8').matchAll(/(?<![\w$.])t\(\s*`/g)].map(() => file.slice(SRC.length))
		);
		expect(dynamic).toEqual([]);
	});
});
