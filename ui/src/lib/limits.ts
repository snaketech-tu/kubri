import type { Usage } from './api';
import { cpuText, memoryText } from './format';
import { t } from './i18n';

export type Measure = 'cpu' | 'memory';

const show = (what: Measure, value: number, unit = false) => (what === 'cpu' ? cpuText(value, unit) : memoryText(value));

/** The most a pod may use (its limit), or what a node can give to pods (its allocatable). */
function ceiling(usage: Usage | undefined, what: Measure): number | undefined {
	return usage?.limits?.[what] ?? usage?.allocatable?.[what];
}

/** The most a pod or node can use, as the list shows it after its use: `/ 500m`; null without one. */
export function limitSuffix(usage: Usage | undefined, what: Measure): string | null {
	const limit = ceiling(usage, what);
	return limit ? `/ ${show(what, limit)}` : null;
}

/** How full a pod is up to its limit, or a node up to its allocatable, from 0 to 1; null without one. */
export function limitShare(usage: Usage | undefined, what: Measure): number | null {
	const limit = ceiling(usage, what);
	if (!usage || !limit) return null;
	return Math.min(1, Math.max(0, usage[what] / limit));
}

/** The request and limit of a pod, or what a node can give, for the tooltip of its CPU or memory cell. */
export function limitsHint(usage: Usage | undefined, what: Measure): string | null {
	if (!usage) return null;
	const request = usage.requests?.[what];
	const limit = usage.limits?.[what];
	const used = show(what, usage[what], true);
	const requested = request ? show(what, request, true) : t('nothing');
	const allocatable = usage.allocatable?.[what];
	if (allocatable) {
		return t('The node uses {used} of the {allocatable} it can give to pods; its pods request {request}', {
			used,
			allocatable: show(what, allocatable, true),
			request: requested
		});
	}
	if (limit) return t('Uses {used}, requests {request}, may use at most {limit}', { used, request: requested, limit: show(what, limit, true) });
	return t('Uses {used}, requests {request}. Not every container has a limit, so there is no maximum.', { used, request: requested });
}

/** One reading of a pod's CPU or memory against its request and limit, for the inspector. */
export interface Reading {
	used: string;
	request: string | null;
	limit: string | null;
	/** How full up to the limit, or up to the request without one, from 0 to 1; null without either. */
	share: number | null;
	/** What the share is measured against. */
	against: 'limit' | 'request' | null;
	/** At the CPU limit (throttled) or close to the memory limit (may be stopped). */
	strained: boolean;
}

export function reading(usage: Usage, what: Measure): Reading {
	const request = usage.requests?.[what];
	const limit = usage.limits?.[what];
	const of = limit ?? request;
	return {
		used: show(what, usage[what], true),
		request: request ? show(what, request, true) : null,
		limit: limit ? show(what, limit, true) : null,
		share: of ? Math.min(1, usage[what] / of) : null,
		against: limit ? 'limit' : request ? 'request' : null,
		strained: !!limit && usage[what] >= limit * 0.9
	};
}
