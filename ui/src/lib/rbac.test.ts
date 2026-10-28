import { describe, expect, it } from 'vitest';
import type { Grant, Rule, SubjectInfo } from './api';
import { allows, filterSubjects, isAdmin, matrix, rulesOf } from './rbac';

function rule(apiGroups: string[], resources: string[], verbs: string[], extra: Partial<Rule> = {}): Rule {
	return { apiGroups, resources, resourceNames: [], nonResourceUrls: [], verbs, ...extra };
}

function grant(namespace: string | null, rules: Rule[]): Grant {
	return {
		bindingKind: namespace ? 'RoleBinding' : 'ClusterRoleBinding',
		binding: 'b',
		roleKind: 'ClusterRole',
		role: 'r',
		namespace,
		roleState: 'found',
		rules
	};
}

describe('rules matrix', () => {
	it('merges the verbs of rules that name the same resource', () => {
		const rows = matrix([rule([''], ['pods'], ['get', 'list']), rule([''], ['pods', 'services'], ['watch'])]);
		expect(rows.map((r) => `${r.group}/${r.resource}`)).toEqual(['core/pods', 'core/services']);
		expect([...rows[0].verbs].sort()).toEqual(['get', 'list', 'watch']);
	});

	it('puts wildcards first and URLs last, keeping odd verbs aside', () => {
		const rows = matrix([
			rule([], [], ['get'], { nonResourceUrls: ['/healthz'] }),
			rule(['apps'], ['deployments'], ['get', 'escalate']),
			rule(['*'], ['*'], ['*'])
		]);
		expect(rows.map((r) => r.resource)).toEqual(['*', 'deployments', '/healthz']);
		expect(rows[1].extras).toEqual(['escalate']);
		expect(allows(rows[0], 'deletecollection')).toBe(true);
		expect(allows(rows[1], 'delete')).toBe(false);
		expect(rows[2].url).toBe(true);
	});

	it('keeps rules limited to some objects apart', () => {
		const rows = matrix([
			rule([''], ['configmaps'], ['get'], { resourceNames: ['b', 'a'] }),
			rule([''], ['configmaps'], ['list'])
		]);
		expect(rows).toHaveLength(2);
		expect(rows.find((r) => r.names.length)?.names).toEqual(['a', 'b']);
	});

	it('reads the rules of a role object', () => {
		const rules = rulesOf({ rules: [{ apiGroups: [''], resources: ['pods'], verbs: ['get'] }, { nonResourceURLs: ['/x'], verbs: ['get'] }] });
		expect(rules[0]).toEqual(rule([''], ['pods'], ['get']));
		expect(rules[1].nonResourceUrls).toEqual(['/x']);
		expect(rulesOf({})).toEqual([]);
	});
});

describe('subjects', () => {
	it('calls only cluster-wide wildcards admin', () => {
		const all = [rule(['*'], ['*'], ['*'])];
		expect(isAdmin([grant(null, all)])).toBe(true);
		expect(isAdmin([grant('shop', all)])).toBe(false);
		expect(isAdmin([grant(null, [rule([''], ['*'], ['*'])])])).toBe(false);
	});

	it('filters by name, kind and namespace', () => {
		const subjects: SubjectInfo[] = [
			{ kind: 'User', name: 'alice', namespace: null, grants: [] },
			{ kind: 'ServiceAccount', name: 'default', namespace: 'shop', grants: [] }
		];
		expect(filterSubjects(subjects, 'ALI').map((s) => s.name)).toEqual(['alice']);
		expect(filterSubjects(subjects, 'shop').map((s) => s.name)).toEqual(['default']);
		expect(filterSubjects(subjects, 'serviceacc')).toHaveLength(1);
		expect(filterSubjects(subjects, ' ')).toHaveLength(2);
	});
});
