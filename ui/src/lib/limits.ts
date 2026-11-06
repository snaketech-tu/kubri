import type { Usage } from './api';
import { cpu, memory } from './format';
import { t } from './i18n';

export type Measure = 'cpu' | 'memory';

const show = (what: Measure, value: number) => (what === 'cpu' ? cpu(value) : memory(value));

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
	const used = show(what, usage[what]);
	const requested = request ? show(what, request) : t('nothing');
	const allocatable = usage.allocatable?.[what];
	if (allocatable) {
		return t('The node uses {used} of the {allocatable} it can give to pods; its pods request {request}', {
			used,
			allocatable: show(what, allocatable),
			request: requested
		});
	}
	if (limit) return t('Uses {used}, requests {request}, may use at most {limit}', { used, request: requested, limit: show(what, limit) });
	return t('Uses {used}, requests {request}. Not every container has a limit, so there is no maximum.', { used, request: requested });
}
