import { describe, expect, it } from 'vitest';
import type { ContextInfo, ResourceInfo } from './api';
import { completions, namespaceCompletions, suggest } from './commands';

function resource(kind: string, plural: string, group = '', shortNames: string[] = [], namespaced = true): ResourceInfo {
	return { group, version: 'v1', plural, kind, singular: kind.toLowerCase(), shortNames, namespaced, verbs: ['list', 'watch'], subresources: [] };
}

const resources = [resource('Pod', 'pods', '', ['po']), resource('Deployment', 'deployments', 'apps', ['deploy']), resource('Node', 'nodes', '', ['no'], false)];
const context = (name: string): ContextInfo => ({ name, cluster: name, server: null, user: null, namespace: null, isDefault: false, added: false });
const contexts = [context('lab'), context('prod-eu'), context('prod')];
const namespaces = ['default', 'kube-node-lease', 'kube-public', 'kube-system', 'shop', 'webshop'];

describe('completions', () => {
	it('puts the exact name first, then names it starts, then names it is part of', () => {
		expect(completions('shop', namespaces)).toEqual(['shop', 'webshop']);
		expect(completions('sh', namespaces)).toEqual(['shop', 'webshop']);
		expect(completions('SH', namespaces)).toEqual(['shop', 'webshop']);
		expect(completions('xyz', namespaces)).toEqual([]);
	});

	it('takes kube-system for kube, and keeps the typed text without known namespaces or for all', () => {
		expect(namespaceCompletions('kube', namespaces)[0]).toBe('kube-system');
		expect(namespaceCompletions('sh', [])).toEqual(['sh']);
		expect(namespaceCompletions('all', namespaces)).toEqual(['all']);
	});
});

describe('the palette completes arguments', () => {
	it('completes the namespace of a list, and offers the others it may mean', () => {
		const s = suggest('po sh', resources, contexts, namespaces);
		expect(s[0]).toMatchObject({ label: 'pods shop', action: { type: 'resource', namespace: 'shop' } });
		expect(s[1]).toMatchObject({ label: 'pods webshop', action: { namespace: 'webshop' } });
		expect(suggest('po kube', resources, contexts, namespaces)[0].action).toMatchObject({ namespace: 'kube-system' });
		// Labels and the context stay as they were typed.
		expect(suggest('po sh app=web', resources, contexts, namespaces)[0]).toMatchObject({
			label: 'pods shop app=web',
			action: { namespace: 'shop', filter: '-l app=web' }
		});
	});

	it('says when no namespace fits instead of opening one', () => {
		const [first] = suggest('po nope', resources, contexts, namespaces);
		expect(first).toMatchObject({ label: 'pods nope', detail: 'No namespace nope', refused: true });
		expect(suggest('helm nope', resources, contexts, namespaces)[0].refused).toBe(true);
	});

	it('leaves cluster-wide kinds and other contexts alone', () => {
		expect(suggest('no', resources, contexts, namespaces)[0]).toMatchObject({ label: 'nodes', action: { type: 'resource' } });
		// The namespaces of another context are not known here; its name is completed.
		expect(suggest('po sh @prod-', resources, contexts, namespaces)[0]).toMatchObject({
			label: 'pods sh @prod-eu',
			action: { namespace: 'sh', context: 'prod-eu' }
		});
	});

	it('completes the namespace of the other views', () => {
		expect(suggest('helm sh', resources, contexts, namespaces)[0].action).toEqual({ type: 'helm', namespace: 'shop' });
		expect(suggest('overview kube', resources, contexts, namespaces)[0].action).toEqual({ type: 'pulses', namespace: 'kube-system' });
		expect(suggest('changes sh 6h', resources, contexts, namespaces)[0].action).toEqual({ type: 'changes', filter: { namespace: 'shop', hours: 6 } });
		expect(suggest('snap sh', resources, contexts, namespaces)[0].action).toEqual({ type: 'snapshot', namespace: 'shop' });
	});

	it('puts contexts that start with the typed text first', () => {
		expect(suggest('ctx prod', resources, contexts, namespaces).map((s) => s.label)).toEqual(['ctx prod', 'ctx prod-eu']);
		expect(suggest('ns sh', resources, contexts, namespaces).map((s) => s.label)).toEqual(['ns shop', 'ns webshop']);
	});
});
