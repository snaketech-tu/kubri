import type { FleetCluster, Health, KindPulse } from './api';

/** How a context is doing on the fleet page, and the words for it. */
export interface FleetState {
	/** Null while it is being checked. */
	health: Health | null;
	/** `Unreachable`, `2 pods failing`, `Healthy`. */
	summary: string;
	/** The API server did not answer: a different problem from failing workloads. */
	unreachable?: boolean;
}

const WORKLOADS = ['Deployment', 'StatefulSet', 'DaemonSet'];

const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? '' : 's'}`;

/** The workloads of the three kinds, counted together. */
export function workloads(kinds: KindPulse[]): KindPulse | null {
	const found = kinds.filter((k) => WORKLOADS.includes(k.kind));
	if (!found.length) return null;
	const listed = found.filter((k) => !k.error);
	return {
		kind: 'Workload',
		resource: found[0].resource,
		total: listed.reduce((n, k) => n + k.total, 0),
		ok: listed.reduce((n, k) => n + k.ok, 0),
		warning: listed.reduce((n, k) => n + k.warning, 0),
		failed: listed.reduce((n, k) => n + k.failed, 0),
		error: listed.length ? null : found[0].error
	};
}

/** Failing nodes count most, then failing workloads and pods; warnings only make it a warning. */
export function fleetState(cluster: FleetCluster | null): FleetState {
	if (!cluster) return { health: null, summary: 'Checking…' };
	if (!cluster.version) return { health: 'failed', summary: 'Unreachable', unreachable: true };
	const nodes = cluster.kinds.find((k) => k.kind === 'Node');
	const pods = cluster.kinds.find((k) => k.kind === 'Pod');
	const work = workloads(cluster.kinds);
	const failed: string[] = [];
	const warned: string[] = [];
	if (nodes?.failed) failed.push(`${plural(nodes.failed, 'node')} not ready`);
	if (work?.failed) failed.push(`${plural(work.failed, 'workload')} failing`);
	if (pods?.failed) failed.push(`${plural(pods.failed, 'pod')} failing`);
	if (nodes?.warning) warned.push(`${plural(nodes.warning, 'node')} cordoned or under pressure`);
	if (work?.warning) warned.push(`${plural(work.warning, 'workload')} not fully ready`);
	if (pods?.warning) warned.push(`${plural(pods.warning, 'pod')} not ready`);
	// Warnings would only lengthen the line; the counts below show them.
	if (failed.length) return { health: 'failed', summary: failed.join(' · ') };
	if (warned.length) return { health: 'warning', summary: warned.join(' · ') };
	if (cluster.error) return { health: 'warning', summary: 'Reachable, but not everything could be listed' };
	if (cluster.kinds.every((k) => k.error)) return { health: 'warning', summary: 'Reachable, nothing may be listed' };
	return { health: 'ok', summary: 'Healthy' };
}

/** A connection error without the layers of the HTTP client it went through. */
export function connectError(message: string): string {
	return message.replace(/^ServiceError: /, '').replace(/client error \(Connect\): /, '').replace(/ \(os error \d+\)/g, '');
}

/** The API server's host, without the scheme. */
export function serverHost(server: string | null): string {
	if (!server) return '';
	try {
		return new URL(server).host;
	} catch {
		return server;
	}
}

/** Runs `task` for each item, at most `limit` at once, in their order. */
export async function eachLimited<T>(items: T[], limit: number, task: (item: T) => Promise<void>) {
	let next = 0;
	const worker = async () => {
		while (next < items.length) await task(items[next++]);
	};
	await Promise.all(Array.from({ length: Math.min(limit, items.length) }, worker));
}
