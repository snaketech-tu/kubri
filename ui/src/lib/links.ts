/**
 * The address a link leads to when it leaves the app: a web page on another site. The desktop
 * window opens no new windows and must not navigate away, so such links go to the browser.
 */
export function externalHref(href: string | null | undefined, origin: string): string | null {
	if (!href) return null;
	let url: URL;
	try {
		url = new URL(href, origin);
	} catch {
		return null;
	}
	if (url.protocol !== 'http:' && url.protocol !== 'https:') return null;
	return url.origin === origin ? null : url.href;
}
