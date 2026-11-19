import { describe, expect, it } from 'vitest';
import type { FavoriteForward, KubeObject } from './api';
import { assignPorts, entriesOf, exportFavorites, forwardKind, parseImport, remotePortOf } from './forward-import';

describe('parseImport from kubectl commands', () => {
	it('reads a script of port-forwards', () => {
		const script = [
			'#!/bin/sh',
			'# shop',
			'kubectl -n shop port-forward svc/api 8081:80 &',
			'kubectl port-forward --namespace=shop deployment/web 3000 &',
			'kubectl --context staging port-forward -n pay service/billing :8080 9090:metrics',
			'echo started',
			'kubectl port-forward pod/db-0 5432:5432 --address 0.0.0.0,::',
			'$ kubectl port-forward redis 6379 -nshop && sleep 1',
			'kubectl.exe port-forward sts/kafka 9092 `',
			'  -n events',
			'wait'
		].join('\n');
		const { entries, problems, format } = parseImport(script);
		expect(format).toBe('kubectl');
		expect(problems).toEqual([]);
		expect(entries.map((e) => [e.kind, e.namespace, e.name, e.remote, e.local, e.address, e.context])).toEqual([
			['services', 'shop', 'api', 80, 8081, '', null],
			['deployments', 'shop', 'web', 3000, 3000, '', null],
			['services', 'pay', 'billing', 8080, null, '', 'staging'],
			['services', 'pay', 'billing', 'metrics', 9090, '', 'staging'],
			['pods', null, 'db-0', 5432, 5432, '0.0.0.0', null],
			['pods', 'shop', 'redis', 6379, 6379, '', null],
			['statefulsets', 'events', 'kafka', 9092, 9092, '', null]
		]);
	});

	it('lets Kubri choose a port that would need admin rights', () => {
		const [e] = parseImport('kubectl port-forward svc/web 80').entries;
		expect(e.remote).toBe(80);
		expect(e.local).toBeNull();
		// Asked for explicitly, it stays.
		expect(parseImport('kubectl port-forward svc/web 443:80').entries[0].local).toBe(443);
	});

	it('says which commands it cannot read', () => {
		const { entries, problems } = parseImport(
			['kubectl port-forward', 'kubectl port-forward ingress/x 80', 'kubectl port-forward svc/a', 'kubectl port-forward svc/a 99999', 'kubectl port-forward svc/b 8080'].join('\n')
		);
		expect(entries.map((e) => e.name)).toEqual(['b']);
		expect(problems.map((p) => p.at)).toEqual([1, 2, 3, 4]);
	});

	it('finds nothing in text without port-forwards', () => {
		expect(parseImport('hello\nworld').format).toBe('none');
		expect(parseImport('   ').format).toBe('none');
	});
});

describe('the favorites file', () => {
	const favorites: FavoriteForward[] = [
		{
			target: { resource: { group: '', version: 'v1', plural: 'services' }, namespace: 'shop', name: 'api' },
			remotePort: 80,
			localPort: 10080,
			address: '',
			startOnConnect: true
		},
		{
			target: { resource: { group: 'apps', version: 'v1', plural: 'deployments' }, namespace: 'shop', name: 'web' },
			remotePort: 3000,
			localPort: 3000,
			address: '0.0.0.0',
			startOnConnect: false
		}
	];

	it('reads back what it exports', () => {
		const text = exportFavorites(favorites);
		expect(text).toContain('target: service/api');
		const { entries, problems, format } = parseImport(text);
		expect(format).toBe('file');
		expect(problems).toEqual([]);
		expect(entries.map((e) => [e.kind, e.namespace, e.name, e.remote, e.local, e.address, e.startOnConnect])).toEqual([
			['services', 'shop', 'api', 80, 10080, '', true],
			['deployments', 'shop', 'web', 3000, 3000, '0.0.0.0', false]
		]);
	});

	it('keeps names and leaves out what is not given', () => {
		const { entries, problems } = parseImport(
			['forwards:', '  - name: Shop API', '    target: svc/api', '    port: http', '  - target: deploy/web', '    port: 3000', '    localPort: 0'].join('\n')
		);
		expect(problems).toEqual([]);
		expect(entries[0]).toMatchObject({ label: 'Shop API', kind: 'services', remote: 'http', local: null, namespace: null, startOnConnect: null });
		expect(entries[1]).toMatchObject({ kind: 'deployments', local: null });
	});

	it('reads JSON and a bare list too', () => {
		expect(parseImport('[{"target":"pod/a","port":80,"localPort":8080}]').entries[0]).toMatchObject({ kind: 'pods', local: 8080 });
	});

	it('says which entries it cannot read', () => {
		const { entries, problems } = parseImport(
			['forwards:', '  - target: api', '    port: 80', '  - target: svc/api', '  - target: svc/api', '    port: 80', '    localPort: 70000', '  - nope'].join('\n')
		);
		expect(entries).toEqual([]);
		expect(problems.map((p) => p.at)).toEqual([1, 2, 3, 4]);
	});
});

describe('forwardKind', () => {
	it('knows kubectl names with and without a group', () => {
		expect(forwardKind('svc')).toBe('services');
		expect(forwardKind('deployments.apps')).toBe('deployments');
		expect(forwardKind('deployments.v1.apps')).toBe('deployments');
		expect(forwardKind('services.v1')).toBe('services');
		expect(forwardKind('deployments.extensions')).toBeNull();
		expect(forwardKind('ingress')).toBeNull();
	});
});

describe('entriesOf', () => {
	it('makes one favorite per TCP port, with the local port an annotation asks for', () => {
		const service = {
			kind: 'Service',
			metadata: { name: 'api', namespace: 'shop' },
			spec: {
				ports: [
					{ name: 'http', port: 80 },
					{ name: 'dns', port: 53, protocol: 'UDP' },
					{ name: 'grpc', port: 9000 }
				]
			}
		} as unknown as KubeObject;
		const pod = {
			kind: 'Pod',
			metadata: { name: 'web-1', namespace: 'shop', annotations: { 'k9scli.io/port-forwards': '9001:http' } },
			spec: { containers: [{ name: 'web', ports: [{ name: 'http', containerPort: 8080 }] }] }
		} as unknown as KubeObject;
		const entries = entriesOf([
			{ kind: 'services', object: service },
			{ kind: 'pods', object: pod }
		]);
		expect(entries.map((e) => [e.name, e.remote, e.local])).toEqual([
			['api', 80, null],
			['api', 9000, null],
			['web-1', 8080, 9001]
		]);
	});
});

describe('remotePortOf', () => {
	const service = { kind: 'Service', metadata: { name: 'api' }, spec: { ports: [{ name: 'http', port: 80 }] } } as unknown as KubeObject;
	it('looks up port names and checks Service ports', () => {
		expect(remotePortOf({ kind: 'services', remote: 'http' }, service)).toBe(80);
		expect(remotePortOf({ kind: 'services', remote: 80 }, service)).toBe(80);
		expect(typeof remotePortOf({ kind: 'services', remote: 81 }, service)).toBe('string');
		expect(typeof remotePortOf({ kind: 'services', remote: 'grpc' }, service)).toBe('string');
		// A pod may listen on a port it does not declare.
		expect(remotePortOf({ kind: 'pods', remote: 81 }, { kind: 'Pod', metadata: { name: 'p' }, spec: {} } as unknown as KubeObject)).toBe(81);
	});
});

describe('assignPorts', () => {
	it('keeps given ports and marks conflicts', () => {
		const plan = assignPorts(
			[
				{ remote: 80, local: 8080 },
				{ remote: 81, local: 8080 },
				{ remote: 82, local: 9000 },
				{ remote: 83, local: 9100 }
			],
			[9000],
			[9100]
		);
		expect(plan.map((p) => [p.local, p.chosen, p.conflict])).toEqual([
			[8080, false, null],
			[8080, false, 'twice'],
			[9000, false, 'favorite'],
			[9100, false, 'inUse']
		]);
	});

	it('chooses the remote port, then plus 10000, then counts from 20000', () => {
		const plan = assignPorts(
			[
				{ remote: 8080, local: null },
				{ remote: 8080, local: null },
				{ remote: 80, local: null },
				{ remote: 80, local: null },
				{ remote: 80, local: null },
				{ remote: 3000, local: 3000 },
				{ remote: 3000, local: null }
			],
			[],
			[20000]
		);
		expect(plan.map((p) => p.local)).toEqual([8080, 18080, 10080, 20001, 20002, 3000, 13000]);
		expect(plan.every((p) => p.conflict === null)).toBe(true);
	});

	it('never chooses a port that is in use or kept', () => {
		const plan = assignPorts([{ remote: 8080, local: null }], [8080], [18080]);
		expect(plan[0]).toEqual({ local: 20000, chosen: true, conflict: null });
	});
});
