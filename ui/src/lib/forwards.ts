import type { PortForwardInfo } from './api';
import { relative } from './format';
import { t } from './i18n';

/** How the last connection through a forward ended, for its row; null before the first one ends. */
export function lastEnd(forward: Pick<PortForwardInfo, 'lastEnd'>, now: number): string | null {
	const end = forward.lastEnd;
	if (!end) return null;
	const when = relative(end.at, now);
	switch (end.by) {
		case 'pod':
			return t('Last connection closed by the pod, {when}', { when });
		case 'client':
			return t('Last connection closed by the client, {when}', { when });
		case 'error':
			return null;
	}
}
