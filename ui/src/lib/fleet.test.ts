import { describe, expect, it } from 'vitest';
import type { FleetCluster, KindPulse } from './api';
import { connectError, eachLimited, fleetState, serverHost, workloads } from './fleet';

const kind = (k: string, ok: number, warning = 0, failed = 0, error: string | null = null): KindPulse => ({
	kind: k,
	resource: { group: '', version: 'v1', plural: `${k.toLowerCase()}s` },
	total: ok + warning + failed,
	ok,
	warning,
	failed,
	error
});

const cluster = (kinds: KindPulse[], extra: Partial<FleetCluster> = {}): FleetCluster => ({
	context: 'prod',
	server: 'https://10.0.0.1:6443',
	version: 'v1.31.2',
	error: null,
	millis: 40,
	kinds,
	cpu: null,
	memory: null,
	...extra
});

describe('fleetState', () => {
	it('says what is wrong, failures first', () => {
		expect(fleetState(null)).toEqual({ health: null, summary: 'Checking…' });
		expect(fleetState(cluster([], { version: null, error: 'timed out' }))).toEqual({ health: 'failed', summary: 'Unreachable' });
		expect(fleetState(cluster([kind('Node', 3), kind('Pod', 40), kind('Deployment', 5)]))).toEqual({ health: 'ok', summary: 'Healthy' });
		expect(fleetState(cluster([kind('Node', 2, 0, 1), kind('Pod', 37, 2, 1)])).summary).toBe('1 node not ready · 1 pod failing');
		expect(fleetState(cluster([kind('Node', 3), kind('Pod', 39, 1)]))).toEqual({ health: 'warning', summary: '1 pod not ready' });
	});

	it('warns when reachable but not listable', () => {
		expect(fleetState(cluster([kind('Node', 0, 0, 0, 'forbidden'), kind('Pod', 0, 0, 0, 'forbidden')])).health).toBe('warning');
		expect(fleetState(cluster([], { error: 'timed out while listing nodes and pods' })).health).toBe('warning');
	});
});

describe('workloads', () => {
	it('adds the three kinds, leaving out those that could not be listed', () => {
		const w = workloads([kind('Deployment', 4, 1), kind('StatefulSet', 1, 0, 1), kind('DaemonSet', 0, 0, 0, 'forbidden'), kind('Pod', 9)]);
		expect(w).toMatchObject({ total: 7, ok: 5, warning: 1, failed: 1, error: null });
		expect(workloads([kind('Pod', 1)])).toBeNull();
	});
});

it('says why a cluster cannot be reached in plain words', () => {
	expect(connectError('ServiceError: client error (Connect): tcp connect error: Connection refused (os error 111)')).toBe(
		'tcp connect error: Connection refused'
	);
});

it('shows the server by host', () => {
	expect(serverHost('https://10.0.0.1:6443')).toBe('10.0.0.1:6443');
	expect(serverHost(null)).toBe('');
});

it('runs at most a few at once', async () => {
	let running = 0;
	let most = 0;
	const done: number[] = [];
	await eachLimited([1, 2, 3, 4, 5], 2, async (n) => {
		running++;
		most = Math.max(most, running);
		await new Promise((r) => setTimeout(r, 5));
		done.push(n);
		running--;
	});
	expect(most).toBe(2);
	expect(done.sort()).toEqual([1, 2, 3, 4, 5]);
});
