import type { SizeAdvice } from './api';

/** Advice in the order it came, with each container's CPU and memory side by side. */
export function byContainer(advice: SizeAdvice[]): { advice: SizeAdvice; first: boolean }[] {
	const key = (a: SizeAdvice) => `${a.kind}/${a.workload.namespace ?? ''}/${a.workload.name}/${a.container}`;
	const groups = new Map<string, SizeAdvice[]>();
	for (const a of advice) {
		const group = groups.get(key(a));
		if (group) group.push(a);
		else groups.set(key(a), [a]);
	}
	return [...groups.values()].flatMap((group) => group.map((advice, i) => ({ advice, first: i === 0 })));
}
