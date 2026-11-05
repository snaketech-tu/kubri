import type { Usage } from './api';
import { cpu, memory } from './format';
import { t } from './i18n';

export type Measure = 'cpu' | 'memory';

const show = (what: Measure, value: number) => (what === 'cpu' ? cpu(value) : memory(value));

/** What a pod may use at most, as the list shows it after its use: `/ 500m`; null without a limit. */
export function limitSuffix(usage: Usage | undefined, what: Measure): string | null {
	const limit = usage?.limits?.[what];
	return limit ? `/ ${show(what, limit)}` : null;
}

/** How full a pod is up to its limit, from 0 to 1; null without a limit. */
export function limitShare(usage: Usage | undefined, what: Measure): number | null {
	const limit = usage?.limits?.[what];
	if (!usage || !limit) return null;
	return Math.min(1, Math.max(0, usage[what] / limit));
}

/** The request and limit of a pod, for the tooltip of its CPU or memory cell. */
export function limitsHint(usage: Usage | undefined, what: Measure): string | null {
	if (!usage) return null;
	const request = usage.requests?.[what];
	const limit = usage.limits?.[what];
	const used = show(what, usage[what]);
	const requested = request ? show(what, request) : t('nothing');
	if (limit) return t('Uses {used}, requests {request}, may use at most {limit}', { used, request: requested, limit: show(what, limit) });
	return t('Uses {used}, requests {request}. Not every container has a limit, so there is no maximum.', { used, request: requested });
}
