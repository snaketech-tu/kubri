export type Tone = 'ok' | 'warn' | 'bad' | '';

const BAD = /(Error|Failed|BackOff|CrashLoop|ImagePull|Evicted|OOMKilled|Lost|Unknown|NotReady|False)/;
const WARN = /(Pending|ContainerCreating|PodInitializing|Terminating|Init:|Waiting|Progressing)/;
const OK = /^(Running|Active|Bound|Succeeded|Completed|Complete|Ready|Available|True|Established)$/;

/** Colors status-like cells, as k9s does for its rows. */
export function cellTone(column: string, value: unknown): Tone {
	if (typeof value !== 'string' || !value) return '';
	const name = column.toLowerCase();
	if (name === 'ready' && /^\d+\/\d+$/.test(value)) {
		const [ready, total] = value.split('/').map(Number);
		return ready === total ? 'ok' : 'warn';
	}
	if (!['status', 'phase', 'state', 'ready', 'available', 'conditions'].includes(name)) return '';
	if (BAD.test(value)) return 'bad';
	if (WARN.test(value)) return 'warn';
	if (OK.test(value) || value.split(',').every((part) => OK.test(part))) return 'ok';
	return '';
}
