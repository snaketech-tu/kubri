/**
 * Feedback opens a new GitHub issue with what Kubri knows already filled in, so a report says
 * which Kubri, and where it runs, without anyone having to look it up.
 */

const NEW_ISSUE = 'https://github.com/snaketech-tu/kubri/issues/new';

/** Kubri's version, from `ui/package.json` when the UI was built. */
export const VERSION: string = typeof __KUBRI_VERSION__ === 'string' ? __KUBRI_VERSION__ : '';

export type FeedbackKind = 'bug' | 'idea';

export const FEEDBACK: Record<FeedbackKind, string> = {
	bug: 'Report a problem',
	idea: 'Suggest an idea'
};

/** The system a browser runs on, from its user agent. */
export function systemOf(userAgent: string): string {
	if (/Windows/.test(userAgent)) return 'Windows';
	if (/iPhone|iPad/.test(userAgent)) return 'iOS';
	if (/Mac OS X|Macintosh/.test(userAgent)) return 'macOS';
	if (/Android/.test(userAgent)) return 'Android';
	if (/CrOS/.test(userAgent)) return 'ChromeOS';
	if (/Linux|X11/.test(userAgent)) return 'Linux';
	return '';
}

/** The browser, from its user agent. Edge and Opera say Chrome too, so they go first. */
export function browserOf(userAgent: string): string {
	if (/Edg\//.test(userAgent)) return 'Edge';
	if (/OPR\//.test(userAgent)) return 'Opera';
	if (/Firefox\//.test(userAgent)) return 'Firefox';
	if (/Chrome\/|Chromium\//.test(userAgent)) return 'Chrome';
	if (/Safari\//.test(userAgent)) return 'Safari';
	return '';
}

/** Where Kubri runs, in the words of the issue form: "Desktop app on Linux", "Browser (Firefox on macOS)". */
export function whereOf(desktop: boolean, userAgent: string): string {
	const system = systemOf(userAgent);
	if (desktop) return system ? `Desktop app on ${system}` : 'Desktop app';
	const browser = [browserOf(userAgent), system].filter(Boolean).join(' on ');
	return browser ? `Browser (${browser})` : 'Browser';
}

/** The address of a new issue of `kind`, with the version and where Kubri runs filled in. */
export function feedbackUrl(kind: FeedbackKind, about: { version: string; where: string }): string {
	const query = new URLSearchParams({ template: kind === 'bug' ? 'bug.yml' : 'idea.yml' });
	if (about.version) query.set('version', about.version);
	if (about.where) query.set('where', about.where);
	return `${NEW_ISSUE}?${query}`;
}

/** The feedback link for this Kubri: `desktop` in the desktop app, else in a browser. */
export function feedbackLink(kind: FeedbackKind, desktop: boolean): string {
	const agent = typeof navigator === 'undefined' ? '' : navigator.userAgent;
	return feedbackUrl(kind, { version: VERSION, where: whereOf(desktop, agent) });
}
