import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import { browserOf, feedbackUrl, systemOf, VERSION, whereOf } from './feedback';

const pkg = JSON.parse(readFileSync(new URL('../../package.json', import.meta.url), 'utf8'));

const FIREFOX_LINUX = 'Mozilla/5.0 (X11; Linux x86_64; rv:140.0) Gecko/20100101 Firefox/140.0';
const EDGE_WINDOWS =
	'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36 Edg/141.0.0.0';
const SAFARI_MAC =
	'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/26.0 Safari/605.1.15';
const DESKTOP_MAC = 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko)';
const CHROME_ANDROID =
	'Mozilla/5.0 (Linux; Android 15; Pixel 9) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Mobile Safari/537.36';

describe('feedback', () => {
	it('knows the version it was built from', () => {
		expect(VERSION).toBe(pkg.version);
	});

	it('names the system and browser', () => {
		expect([systemOf(FIREFOX_LINUX), browserOf(FIREFOX_LINUX)]).toEqual(['Linux', 'Firefox']);
		expect([systemOf(EDGE_WINDOWS), browserOf(EDGE_WINDOWS)]).toEqual(['Windows', 'Edge']);
		expect([systemOf(SAFARI_MAC), browserOf(SAFARI_MAC)]).toEqual(['macOS', 'Safari']);
		expect([systemOf(CHROME_ANDROID), browserOf(CHROME_ANDROID)]).toEqual(['Android', 'Chrome']);
		expect([systemOf('curl/8'), browserOf('curl/8')]).toEqual(['', '']);
	});

	it('says where Kubri runs', () => {
		expect(whereOf(true, DESKTOP_MAC)).toBe('Desktop app on macOS');
		expect(whereOf(false, FIREFOX_LINUX)).toBe('Browser (Firefox on Linux)');
		expect(whereOf(false, 'curl/8')).toBe('Browser');
		expect(whereOf(true, 'curl/8')).toBe('Desktop app');
	});

	it('opens the right form with what is known filled in', () => {
		const url = new URL(feedbackUrl('bug', { version: '1.7.0', where: 'Desktop app on Linux' }));
		expect(url.origin + url.pathname).toBe('https://github.com/snaketech-tu/kubri/issues/new');
		expect(Object.fromEntries(url.searchParams)).toEqual({
			template: 'bug.yml',
			version: '1.7.0',
			where: 'Desktop app on Linux'
		});
		const idea = new URL(feedbackUrl('idea', { version: '', where: '' }));
		expect(Object.fromEntries(idea.searchParams)).toEqual({ template: 'idea.yml' });
	});
});
