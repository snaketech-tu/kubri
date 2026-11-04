import type { ResourceRef } from './api';

/**
 * Where the web app is, in the address bar: the context and the view in it, such as
 * `#/lab/resources/deployments.v1.apps?ns=shop`. A reload opens the same view again.
 */
export interface Address<Page extends string, Scope> {
	context: string;
	page: Page;
	resource: ResourceRef | null;
	/** Empty for all namespaces. */
	namespace: string;
	scope: Scope | null;
	xray: ResourceRef | null;
}

/** `deployments.v1.apps`, or `pods.v1` for the core group, as kubectl writes kinds. */
function refText(ref: ResourceRef): string {
	return [ref.plural, ref.version, ref.group].filter(Boolean).join('.');
}

function refOf(text: string | null): ResourceRef | null {
	if (!text) return null;
	const [plural, version, ...group] = text.split('.');
	return plural && version ? { plural, version, group: group.join('.') } : null;
}

export function toHash<Page extends string, Scope>(address: Address<Page, Scope>): string {
	const path = [address.context, address.page];
	if (address.resource) path.push(refText(address.resource));
	const params = new URLSearchParams();
	if (address.namespace) params.set('ns', address.namespace);
	if (address.xray) params.set('xray', refText(address.xray));
	if (address.scope) params.set('scope', JSON.stringify(address.scope));
	const query = params.toString();
	return `#/${path.map(encodeURIComponent).join('/')}${query ? `?${query}` : ''}`;
}

/** The address in `hash`, if it is one Kubri wrote and `pages` has its page. */
export function fromHash<Page extends string, Scope>(
	hash: string,
	pages: readonly Page[]
): Address<Page, Scope> | null {
	if (!hash.startsWith('#/')) return null;
	const at = hash.indexOf('?');
	const path = at === -1 ? hash.slice(2) : hash.slice(2, at);
	const params = new URLSearchParams(at === -1 ? '' : hash.slice(at + 1));
	let parts: string[];
	try {
		parts = path.split('/').map(decodeURIComponent);
	} catch {
		return null;
	}
	const [context, page, resource] = parts;
	if (!context || !pages.includes(page as Page)) return null;
	let scope: Scope | null = null;
	try {
		const parsed = JSON.parse(params.get('scope') ?? 'null');
		scope = parsed && typeof parsed === 'object' && !Array.isArray(parsed) ? parsed : null;
	} catch {
		// A scope that does not parse is left out; the list shows everything.
	}
	return {
		context,
		page: page as Page,
		resource: refOf(resource ?? null),
		namespace: params.get('ns') ?? '',
		scope,
		xray: refOf(params.get('xray'))
	};
}
