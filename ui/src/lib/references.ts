import type { Reference } from './api';

/** `3 Deployments`, `1 Ingress`: a count with the kind, plural when needed. */
export function counted(n: number, kind: string): string {
	return `${n} ${n === 1 ? kind : kind.endsWith('s') ? `${kind}es` : `${kind}s`}`;
}

/** What uses an object, counted by kind: `3 Deployments, 1 Pod and 1 Ingress for TLS`. */
export function summarizeUsers(refs: Reference[]): string {
	const counts = new Map<string, { kind: string; how?: string; n: number }>();
	for (const r of refs) {
		const key = `${r.kind}/${r.how ?? ''}`;
		const entry = counts.get(key) ?? { kind: r.kind, how: r.how, n: 0 };
		entry.n++;
		counts.set(key, entry);
	}
	const parts = [...counts.values()].map((c) => `${counted(c.n, c.kind)}${c.how ? ` for ${c.how}` : ''}`);
	return parts.length > 1 ? `${parts.slice(0, -1).join(', ')} and ${parts.at(-1)}` : (parts[0] ?? '');
}
