/**
 * Grafana links of a context: dashboard URLs copied from the browser become templates whose
 * placeholders each object fills in.
 */

/** Dashboard variables Kubri fills, by the placeholder each one gets. */
const VARIABLES: Record<string, string> = {
	namespace: 'namespace',
	pod: 'pod',
	node: 'node',
	workload: 'workload',
	deployment: 'deployment',
	statefulset: 'statefulset',
	daemonset: 'daemonset',
	replicaset: 'replicaset',
	job: 'job',
	cronjob: 'cronjob',
	service: 'service',
	ingress: 'ingress',
	type: 'kind',
	workload_type: 'kind'
};

/** The placeholders in a template, like `pod` for `{pod}`, each once. */
export function placeholders(template: string): string[] {
	return [...new Set([...template.matchAll(/\{([a-z0-9_]+)\}/g)].map((m) => m[1]))];
}

/**
 * A dashboard URL from the browser, like `…/d/abc/pods?var-namespace=shop&var-pod=web-1`,
 * with the values of the variables Kubri knows replaced by placeholders, and a fixed time
 * range dropped so that the dashboard opens on its own. Anything that is no URL, or already
 * has placeholders, comes back as it is.
 */
export function templateFromUrl(text: string): string {
	const url = text.trim();
	if (/\{[a-z0-9_]+\}/.test(url)) return url;
	let parsed: URL;
	try {
		parsed = new URL(url);
	} catch {
		return url;
	}
	if (!['http:', 'https:'].includes(parsed.protocol) || !parsed.search) return url;
	const parts: string[] = [];
	for (const [key, value] of parsed.searchParams) {
		const variable = key.startsWith('var-') ? VARIABLES[key.slice(4).toLowerCase()] : undefined;
		// Absolute times would show the moment the link was copied.
		if ((key === 'from' || key === 'to') && /^\d+$/.test(value)) continue;
		parts.push(`${encodeURIComponent(key)}=${variable ? `{${variable}}` : encodeURIComponent(value)}`);
	}
	return `${parsed.origin}${parsed.pathname}${parts.length ? `?${parts.join('&')}` : ''}${parsed.hash}`;
}

/** A title from a dashboard URL's slug, like `Pods` for `/d/abc/pods`. */
export function titleFromUrl(text: string): string {
	try {
		const slug = new URL(text.trim()).pathname.match(/\/d\/[^/]+\/([^/]+)/)?.[1];
		if (!slug) return '';
		const words = decodeURIComponent(slug).replace(/[-_]+/g, ' ').trim();
		return words ? words[0].toUpperCase() + words.slice(1) : '';
	} catch {
		return '';
	}
}

/** Which objects a template shows for, in words. */
export function fitsText(template: string): string {
	const names = placeholders(template);
	const kinds = names.filter((n) => !['namespace', 'name', 'kind', 'context', 'workload'].includes(n));
	if (kinds.length > 1) return `Shows for no object: it needs both ${kinds.join(' and ')}`;
	if (kinds.length === 1) return `Shows for ${kinds[0]}${kinds[0].endsWith('s') ? 'es' : 's'}`;
	if (names.includes('workload')) return 'Shows for workloads';
	if (names.includes('namespace')) return 'Shows for namespaces and everything in them';
	return 'Shows for every object';
}
