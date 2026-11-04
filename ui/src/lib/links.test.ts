import { describe, expect, it } from 'vitest';
import { externalHref } from './links';

describe('externalHref', () => {
	const origin = 'tauri://localhost';

	it('sends web pages on other sites to the browser', () => {
		expect(externalHref('https://prometheus.io/docs/', origin)).toBe('https://prometheus.io/docs/');
		expect(externalHref('http://127.0.0.1:8080', origin)).toBe('http://127.0.0.1:8080/');
	});

	it('keeps links within the app and other schemes', () => {
		expect(externalHref('/integrations', origin)).toBeNull();
		expect(externalHref('#top', origin)).toBeNull();
		expect(externalHref('http://tauri.localhost/x', 'http://tauri.localhost')).toBeNull();
		expect(externalHref('mailto:a@example.com', origin)).toBeNull();
		expect(externalHref('javascript:alert(1)', origin)).toBeNull();
		expect(externalHref('', origin)).toBeNull();
		expect(externalHref(null, origin)).toBeNull();
	});
});
