import { describe, expect, it } from 'vitest';
import type { ContextInfo, ResourceInfo } from './api';
import { fieldTest, parseSearch, remember, unbounded, words } from './search';

const resource = (group: string, plural: string, kind: string, shortNames: string[] = []): ResourceInfo => ({
	group,
	version: 'v1',
	plural,
	kind,
	singular: kind.toLowerCase(),
	shortNames,
	namespaced: true,
	verbs: ['list', 'watch', 'get'],
	subresources: []
});
const resources = [resource('', 'pods', 'Pod', ['po']), resource('apps', 'deployments', 'Deployment', ['deploy']), resource('', 'services', 'Service', ['svc'])];
const contexts: ContextInfo[] = ['prod', 'staging'].map((name) => ({ name, cluster: null, user: null, namespace: null, isDefault: false, added: false }));

describe('parseSearch', () => {
	it('reads names, kinds, namespaces, labels, fields and contexts', () => {
		const { request, problems } = parseSearch('web kind:deploy,svc ns:shop -l tier=front status.phase!=Running ctx:*', resources, contexts);
		expect(problems).toEqual([]);
		expect(request).toEqual({
			text: 'web',
			kinds: [
				{ group: 'apps', version: 'v1', plural: 'deployments' },
				{ group: '', version: 'v1', plural: 'services' }
			],
			allKinds: false,
			namespace: 'shop',
			labels: 'tier=front',
			fields: [{ path: 'status.phase', op: '!=', value: 'Running' }],
			contexts: ['prod', 'staging']
		});
	});

	it('says what it did not understand', () => {
		const { request, problems } = parseSearch('kind:widgets ctx:dev in:all -lapp=web label:tier=db', resources, contexts);
		expect(problems).toEqual(['No kind is called widgets', 'No context is called dev']);
		expect(request.allKinds).toBe(true);
		expect(request.labels).toBe('app=web,tier=db');
	});

	it('keeps label words apart from field tests', () => {
		expect(fieldTest('app=web')).toBeNull();
		expect(fieldTest('spec.replicas>=2')).toEqual({ path: 'spec.replicas', op: '>=', value: '2' });
		expect(fieldTest("metadata.labels['app.kubernetes.io/name']=shop")).toEqual({
			path: "metadata.labels['app.kubernetes.io/name']",
			op: '=',
			value: 'shop'
		});
		expect(words(`"my app" /^web-\\d/`)).toEqual(['my app', '/^web-\\d/']);
	});

	it('knows a search that would list everything', () => {
		expect(unbounded(parseSearch('', resources, contexts).request)).toBe(true);
		expect(unbounded(parseSearch('kind:po', resources, contexts).request)).toBe(false);
	});
});

it('remembers recent objects, newest first and once each', () => {
	const o = (name: string) => ({ context: 'prod', kind: 'Pod', group: '', version: 'v1', plural: 'pods', namespace: 'shop', name });
	let list = remember(o('a'), []);
	list = remember(o('b'), list);
	list = remember(o('a'), list);
	expect(list.map((x) => x.name)).toEqual(['a', 'b']);
});
