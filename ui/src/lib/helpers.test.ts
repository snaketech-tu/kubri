import { describe, expect, it } from 'vitest';
import type { ContextInfo, ResourceInfo, Row } from './api';
import { matchResources, namesAction, suggest } from './commands';
import { age, cpu, imageParts, isNotFound, memory, workloadPrefix } from './format';
import { cellTone } from './status';
import { filterRows, hasHealth, isFaulty, sortRows, toCsv } from './table';

function resource(kind: string, plural: string, group = '', shortNames: string[] = []): ResourceInfo {
	return {
		group,
		version: 'v1',
		plural,
		kind,
		singular: kind.toLowerCase(),
		shortNames,
		namespaced: true,
		verbs: ['list', 'watch'],
		subresources: []
	};
}

const resources = [
	resource('Pod', 'pods', '', ['po']),
	resource('Deployment', 'deployments', 'apps', ['deploy']),
	resource('Event', 'events', 'events.k8s.io'),
	resource('Event', 'events', '', ['ev']),
	resource('PodDisruptionBudget', 'poddisruptionbudgets', 'policy', ['pdb']),
	resource('Endpoints', 'endpoints')
];

const contexts: ContextInfo[] = [
	{ name: 'prod', cluster: 'prod', user: null, namespace: 'web', isDefault: true, added: false },
	{ name: 'staging', cluster: 'stage', user: null, namespace: null, isDefault: false, added: false }
];

describe('command palette', () => {
	it('opens the port-forward list', () => {
		expect(suggest('pf', resources, contexts, [])[0].action).toEqual({ type: 'portforwards' });
		expect(suggest(':portforwards', resources, contexts, [])).toHaveLength(1);
	});

	it('resolves short names exactly', () => {
		expect(matchResources('po', resources)[0].plural).toBe('pods');
		expect(matchResources('deploy', resources)[0].kind).toBe('Deployment');
	});

	it('does not take two letters inside a name for a match', () => {
		expect(matchResources('dp', resources).map((r) => r.plural)).not.toContain('endpoints');
		expect(matchResources('ploy', resources)[0].kind).toBe('Deployment');
	});

	it('prefers the core group for ambiguous names', () => {
		expect(matchResources('events', resources)[0].group).toBe('');
	});

	it('accepts group-qualified names', () => {
		expect(matchResources('events.events.k8s.io', resources)[0].group).toBe('events.k8s.io');
	});

	it('lists contexts for ctx', () => {
		const s = suggest(':ctx ', resources, contexts, []);
		expect(s.map((x) => x.action)).toEqual([
			{ type: 'context', name: 'prod' },
			{ type: 'context', name: 'staging' }
		]);
		expect(suggest('ctx sta', resources, contexts, [])[0].action).toEqual({ type: 'context', name: 'staging' });
	});

	it('opens the context list for a bare ctx instead of switching to the first context', () => {
		const s = suggest(':ctx', resources, contexts, []);
		expect(s[0].action).toEqual({ type: 'fleet' });
		expect(s.slice(1).map((x) => x.action)).toEqual([
			{ type: 'context', name: 'prod' },
			{ type: 'context', name: 'staging' }
		]);
	});

	it('switches namespaces with ns', () => {
		const s = suggest('ns kube', resources, contexts, ['default', 'kube-system']);
		expect(s[0].action).toEqual({ type: 'namespace', name: 'kube-system' });
		expect(suggest('ns ', resources, contexts, ['default'])[0].action).toEqual({ type: 'namespace', name: '' });
	});

	it('opens pulses, optionally in a namespace', () => {
		expect(suggest('pulses', resources, contexts, [])[0].action).toEqual({ type: 'pulses', namespace: undefined });
		expect(suggest(':pu shop', resources, contexts, [])[0].action).toEqual({ type: 'pulses', namespace: 'shop' });
	});

	it('names each view once and keeps the k9s commands as aliases', () => {
		for (const [typed, type] of [
			['overview', 'pulses'],
			['pulses', 'pulses'],
			['access', 'rbac'],
			['rbac', 'rbac'],
			['checks', 'sanitizer'],
			['popeye', 'sanitizer']
		]) {
			const [first] = suggest(typed, resources, contexts, []);
			expect(first.action).toEqual({ type, namespace: undefined });
			expect(['overview', 'access', 'checks']).toContain(first.label);
		}
		expect(suggest('popeye shop', resources, contexts, [])[0].label).toBe('checks shop');
	});

	it('opens the Tools and All kinds pages', () => {
		expect(suggest('tools', resources, contexts, [])[0].action).toEqual({ type: 'tools' });
		expect(suggest('kinds', resources, contexts, [])[0].action).toEqual({ type: 'kinds', filter: undefined });
		expect(suggest(':kinds cert', resources, contexts, [])[0].action).toEqual({ type: 'kinds', filter: 'cert' });
		expect(suggest('api-resources', resources, contexts, [])[0].action).toEqual({ type: 'kinds', filter: undefined });
	});

	it('opens xray for the kinds it supports', () => {
		const [first, ...rest] = suggest('xray deploy kube-system', resources, contexts, []);
		expect(first.action).toMatchObject({ type: 'xray', namespace: 'kube-system' });
		expect(first.label).toBe('xray deployments.apps kube-system');
		expect(rest).toHaveLength(0);
		// Kinds XRay cannot start from are not offered.
		expect(suggest('xray', resources, contexts, []).map((s) => s.label)).toEqual(['xray deployments.apps', 'xray pods']);
	});

	it('opens What changed for a namespace and a range', () => {
		expect(suggest(':changes shop 6h', resources, contexts, [])[0]).toEqual(
			expect.objectContaining({ label: 'changes shop 6h', action: { type: 'changes', filter: { namespace: 'shop', hours: 6 } } })
		);
		expect(suggest('history', resources, contexts, []).map((s) => s.label)).toEqual(['changes', 'changes 1h']);
		expect(suggest('changes 30m', resources, contexts, [])[0].label).toBe('changes 30m');
		expect(suggest('chan', resources, contexts, []).map((s) => s.label)).toContain('changes 1h');

		// `snapshot` saves the current namespace, `snapshot all` the cluster.
		expect(suggest(':snapshot', resources, contexts, []).map((s) => s.action)).toEqual([
			{ type: 'snapshot', namespace: undefined },
			{ type: 'snapshot', namespace: null }
		]);
		expect(suggest('snap shop', resources, contexts, [])).toEqual([
			expect.objectContaining({ label: 'snapshot shop', action: { type: 'snapshot', namespace: 'shop' } })
		]);
		expect(suggest('snaps', resources, contexts, []).map((s) => s.label)).toContain('snapshot');
		expect(suggest(':kubeconfig', resources, contexts, []).map((s) => s.action)).toEqual([{ type: 'kubeconfig' }]);
	});

	it('searches with everything after the command', () => {
		expect(suggest(':search web ctx:*', resources, contexts, [])).toEqual([
			expect.objectContaining({ action: { type: 'search', query: 'web ctx:*' } })
		]);
		expect(suggest('find', resources, contexts, [])[0].action).toEqual({ type: 'search', query: '' });
		expect(suggest('sea', resources, contexts, []).map((s) => s.label)).toContain('search');
	});

	it('opens rbac and helm, optionally in a namespace', () => {
		expect(suggest('rbac', resources, contexts, [])[0].action).toEqual({ type: 'rbac', namespace: undefined });
		expect(suggest(':policy shop', resources, contexts, [])[0].action).toEqual({ type: 'rbac', namespace: 'shop' });
		expect(suggest('helm', resources, contexts, [])[0].action).toEqual({ type: 'helm', namespace: undefined });
		expect(suggest('charts shop', resources, contexts, [])[0].action).toEqual({ type: 'helm', namespace: 'shop' });
		// Offered while typed, after resources that match.
		expect(suggest('he', resources, contexts, []).map((s) => s.label)).toContain('helm');
	});

	it('opens the sanitizer and the benchmarks', () => {
		expect(suggest('popeye kube-system', resources, contexts, [])[0].action).toEqual({
			type: 'sanitizer',
			namespace: 'kube-system'
		});
		expect(suggest(':pop', resources, contexts, [])[0].action).toEqual({ type: 'sanitizer', namespace: undefined });
		expect(suggest('be', resources, contexts, [])[0].action).toEqual({ type: 'benchmarks' });
	});

	it('opens a local terminal where it is allowed', () => {
		expect(suggest(':term', resources, contexts, [])[0].action).toEqual({ type: 'terminal' });
		expect(suggest('terminal new', resources, contexts, []).map((s) => s.action)).toEqual([{ type: 'terminal', another: true }]);
		expect(suggest('termi', resources, contexts, []).map((s) => s.action)).toContainEqual({ type: 'terminal' });
		const off = { aliases: [], skins: [], terminal: false };
		expect(suggest('term', resources, contexts, [], off).some((s) => s.action.type === 'terminal')).toBe(false);
		expect(suggest('termi', resources, contexts, [], off).some((s) => s.action.type === 'terminal')).toBe(false);
	});

	it('passes a namespace argument to resource commands', () => {
		const s = suggest('po kube-system', resources, contexts, []);
		expect(s[0].action).toMatchObject({ type: 'resource', namespace: 'kube-system' });
	});
});

describe('namesAction', () => {
	it('puts an action first when its verb is typed', () => {
		expect(namesAction('Restart', 'restart', resources)).toBe(true);
		expect(namesAction('Scale…', ' Scale ', resources)).toBe(true);
		expect(namesAction('Copy name', 'copy', resources)).toBe(true);
	});

	it('leaves kinds, aliases and parts of words alone', () => {
		expect(namesAction('Events', 'events', resources)).toBe(false);
		expect(namesAction('Port-forward…', 'po', resources)).toBe(false);
		expect(namesAction('Restart', 'rest', resources)).toBe(false);
		expect(namesAction('Copy name', 'copy name', resources)).toBe(false);
		expect(namesAction('Restart', '', resources)).toBe(false);
	});
});

describe('formatting', () => {
	const now = Date.parse('2026-10-27T12:00:00Z');
	it('formats ages like kubectl', () => {
		expect(age('2026-10-27T11:59:30Z', now)).toBe('30s');
		expect(age('2026-10-27T10:00:00Z', now)).toBe('2h');
		expect(age('2026-10-20T12:00:00Z', now)).toBe('7d');
		expect(age(null, now)).toBe('');
	});

	it('formats usage', () => {
		expect(cpu(105.4)).toBe('105m');
		expect(memory(80 * 1024 * 1024)).toBe('80Mi');
		expect(memory(1.5 * 1024 ** 3)).toBe('1.5Gi');
	});
});

describe('status colors', () => {
	it('colors pod states', () => {
		expect(cellTone('Status', 'Running')).toBe('ok');
		expect(cellTone('Status', 'CrashLoopBackOff')).toBe('bad');
		expect(cellTone('Status', 'Pending')).toBe('warn');
		expect(cellTone('Ready', '1/2')).toBe('warn');
		expect(cellTone('Ready', '2/2')).toBe('ok');
		expect(cellTone('Name', 'Running')).toBe('');
		expect(cellTone('Type', 'Warning')).toBe('warn');
		expect(cellTone('Type', 'Normal')).toBe('');
	});

	it('colors the health of views by its state, not its message', () => {
		expect(cellTone('Health', 'Healthy')).toBe('ok');
		expect(cellTone('Health', 'Failed: Ready is True elsewhere')).toBe('bad');
		expect(cellTone('Health', 'Warning: Suspended')).toBe('warn');
		expect(cellTone('Health', 'Warning: secret not found, Failed to fetch')).toBe('warn');
	});
});

function row(name: string, namespace: string, cells: unknown[], created = '2026-10-01T00:00:00Z'): Row {
	return { key: `${namespace}/${name}`, name, namespace, created, labels: { app: name }, owners: [], cells };
}

describe('table rows', () => {
	const columns = [
		{ name: 'Name', type: 'string', format: 'name', description: '', priority: 0 },
		{ name: 'Restarts', type: 'string', format: '', description: '', priority: 0 },
		{ name: 'Age', type: 'string', format: '', description: '', priority: 0 }
	];
	const rows = [
		row('b', 'x', ['b', '10 (3m ago)', '1d'], '2026-10-02T00:00:00Z'),
		row('a', 'y', ['a', '2', '2d'], '2026-10-01T00:00:00Z'),
		row('c', 'x', ['c', '0', '1h'], '2026-10-03T00:00:00Z')
	];

	it('sorts by namespace and name by default', () => {
		expect(sortRows(rows, columns, { column: -1, descending: false }).map((r) => r.name)).toEqual(['b', 'c', 'a']);
	});

	it('sorts restarts numerically and age by creation time', () => {
		expect(sortRows(rows, columns, { column: 1, descending: true }).map((r) => r.name)).toEqual(['b', 'a', 'c']);
		expect(sortRows(rows, columns, { column: 2, descending: false }).map((r) => r.name)).toEqual(['c', 'b', 'a']);
	});

	it('sorts durations and quantities by their size', () => {
		const column = (name: string) => ({ name, type: 'string', format: '', description: '', priority: 0 });
		const sorted = (name: string, values: string[]) =>
			sortRows(
				values.map((v) => row(v, 'x', [v])),
				[column(name)],
				{ column: 0, descending: false }
			).map((r) => r.name);
		expect(sorted('Last Seen', ['13m', '5m52s', '45s', '2d', '1h', '3y120d'])).toEqual([
			'45s',
			'5m52s',
			'13m',
			'1h',
			'2d',
			'3y120d'
		]);
		expect(sorted('CPU', ['1500m', '2', '100m', '1'])).toEqual(['100m', '1', '1500m', '2']);
		expect(sorted('Mem', ['1Gi', '262144k', '512Mi'])).toEqual(['262144k', '512Mi', '1Gi']);
		// Values that are not numbers come after the numbers, whatever their text.
		expect(sorted('Mem', ['<none>', '100', '64Mi', '<unknown>', '2'])).toEqual([
			'2',
			'100',
			'64Mi',
			'<none>',
			'<unknown>'
		]);
	});

	it('filters by text and by label', () => {
		expect(filterRows(rows, 'y').map((r) => r.name)).toEqual(['a']);
		expect(filterRows(rows, '-l app=c').map((r) => r.name)).toEqual(['c']);
	});

	it('filters like k9s: regular expressions, inverse and fuzzy', () => {
		const names = (filter: string, list = rows) => filterRows(list, filter).map((r) => r.name);
		expect(names('^[ab]$')).toEqual(['b', 'a']);
		// Between slashes, as the placeholder shows, and as `!/…/`.
		expect(names('/^[ab]$/')).toEqual(['b', 'a']);
		expect(names('!/^[ab]$/')).toEqual(['c']);
		expect(names('2D')).toEqual(['a']);
		expect(names('!x')).toEqual(['a']);
		expect(names('! ^c$')).toEqual(['b', 'a']);
		// Not a valid expression yet, as while typing: taken literally.
		expect(names('10 (')).toEqual(['b']);
		const pods = [row('web-7d4f9', 'shop', []), row('worker-1', 'shop', []), row('api', 'shop', [])];
		expect(names('-f wb', pods)).toEqual(['web-7d4f9']);
		expect(names('-fwr', pods)).toEqual(['worker-1']);
	});

	it('finds faulty rows', () => {
		const columns = ['Name', 'Ready', 'Status'].map((name) => ({
			name,
			type: 'string',
			format: '',
			description: '',
			priority: 0
		}));
		const faulty = (cells: string[]) => isFaulty(columns, row(cells[0], 'x', cells));
		expect(faulty(['a', '1/1', 'Running'])).toBe(false);
		expect(faulty(['a', '0/1', 'Running'])).toBe(true);
		expect(faulty(['a', '0/1', 'CrashLoopBackOff'])).toBe(true);
		expect(faulty(['a', '0/1', 'Pending'])).toBe(true);
		// A finished pod is not ready, and that is fine.
		expect(faulty(['a', '0/1', 'Completed'])).toBe(false);
	});

	it('writes CSV', () => {
		expect(toCsv(['Name', 'Labels'], [['web', 'a=1,b=2'], ['say "hi"', '']])).toBe(
			'Name,Labels\nweb,"a=1,b=2"\n"say ""hi""",\n'
		);
	});
});

describe('workloadPrefix', () => {
	it('names the workload a generated pod name came from', () => {
		expect(workloadPrefix('frontend-6f8dcb7dd4-c5zl6')).toBe('frontend');
		expect(workloadPrefix('shop-api-6f8dcb7dd4-c5zl6')).toBe('shop-api');
	});
	it('has none for names that are not generated', () => {
		expect(workloadPrefix('db-0')).toBeNull();
		expect(workloadPrefix('gpu-worker')).toBeNull();
	});
});

describe('isNotFound', () => {
	it('recognises the API server wording', () => {
		expect(isNotFound('pods "x" not found')).toBe(true);
		expect(isNotFound('forbidden')).toBe(false);
	});
});

describe('hasHealth', () => {
	const columns = (...names: string[]) => names.map((name) => ({ name, type: 'string', format: '', description: '', priority: 0 }));

	it('is true for kinds with a state, readiness or health', () => {
		expect(hasHealth(columns('Name', 'Ready', 'Status', 'Age'), 'pods')).toBe(true);
		expect(hasHealth(columns('Name', 'Ready', 'Up-to-date', 'Available'), 'deployments')).toBe(true);
		expect(hasHealth(columns('Name', 'Health'), 'widgets')).toBe(true);
		expect(hasHealth(columns('Last Seen', 'Type', 'Reason'), 'events')).toBe(true);
	});

	it('is false for kinds with nothing to go wrong in their rows', () => {
		expect(hasHealth(columns('Name', 'Type', 'Data', 'Age'), 'secrets')).toBe(false);
		expect(hasHealth(columns('Name', 'Class', 'Hosts', 'Address', 'Ports', 'Age'), 'ingresses')).toBe(false);
	});
});

describe('imageParts', () => {
	it('breaks an image after its separators', () => {
		expect(imageParts('ghcr.io/org/api:v2.1')).toEqual(['ghcr.io/', 'org/', 'api:', 'v2.1']);
		expect(imageParts('busybox')).toEqual(['busybox']);
	});

	it('cuts a digest short', () => {
		const digest = '4f2a9c1be07d5e8a3c6b1f0e9d8c7b6a5f4e3d2c1b0a9f8e7d6c5b4a3f2e1d0c';
		expect(imageParts(`nginx:1.27@sha256:${digest}`).join('')).toBe('nginx:1.27@sha256:4f2a9c1be07d…');
	});
});
