import type { AccessCheck, Grant, Identity, ResourceInfo, Rule, SubjectInfo } from './api';

/** The verbs that get a column of their own, in the order kubectl lists them. */
export const VERBS = ['get', 'list', 'watch', 'create', 'update', 'patch', 'delete', 'deletecollection'] as const;

export interface MatrixRow {
	key: string;
	/** `core` for the core group, `*` for every group, empty for URLs. */
	group: string;
	/** A resource such as `pods`, `*`, or a URL such as `/healthz`. */
	resource: string;
	/** Set when the rule is limited to some objects. */
	names: string[];
	url: boolean;
	verbs: Set<string>;
	/** Verbs without a column, such as `impersonate` or `escalate`. */
	extras: string[];
}

/** One row per resource, merging the verbs of every rule that names it, as k9s shows roles. */
export function matrix(rules: Rule[]): MatrixRow[] {
	const rows = new Map<string, MatrixRow>();
	const add = (group: string, resource: string, names: string[], url: boolean, verbs: string[]) => {
		const key = `${group}|${resource}|${names.join(',')}`;
		const row = rows.get(key) ?? { key, group, resource, names, url, verbs: new Set<string>(), extras: [] };
		for (const v of verbs) row.verbs.add(v);
		rows.set(key, row);
	};
	for (const rule of rules) {
		for (const url of rule.nonResourceUrls) add('', url, [], true, rule.verbs);
		const groups = rule.apiGroups.length ? rule.apiGroups : [''];
		for (const group of groups) {
			for (const resource of rule.resources) {
				add(group === '' ? 'core' : group, resource, [...rule.resourceNames].sort(), false, rule.verbs);
			}
		}
	}
	const known = new Set<string>(VERBS);
	for (const row of rows.values()) row.extras = [...row.verbs].filter((v) => !known.has(v)).sort();
	// Wildcards first: they are what a reader most needs to notice.
	const rank = (r: MatrixRow) => (r.url ? 2 : r.resource === '*' || r.group === '*' ? 0 : 1);
	return [...rows.values()].sort(
		(a, b) => rank(a) - rank(b) || a.group.localeCompare(b.group) || a.resource.localeCompare(b.resource)
	);
}

export function allows(row: MatrixRow, verb: string) {
	return row.verbs.has(verb) || row.verbs.has('*');
}

/** Turns the `rules` of a Role or ClusterRole object into rules. */
export function rulesOf(object: object): Rule[] {
	const value = (object as { rules?: unknown }).rules;
	const rules = Array.isArray(value) ? value : [];
	return rules.map((r: any) => ({
		apiGroups: r.apiGroups ?? [],
		resources: r.resources ?? [],
		resourceNames: r.resourceNames ?? [],
		nonResourceUrls: r.nonResourceURLs ?? [],
		verbs: r.verbs ?? []
	}));
}

export function subjectLabel(s: Pick<SubjectInfo, 'kind' | 'name' | 'namespace'>) {
	return s.namespace ? `${s.namespace}/${s.name}` : s.name;
}

const PLURALS: Record<string, string> = {
	Role: 'roles',
	ClusterRole: 'clusterroles',
	RoleBinding: 'rolebindings',
	ClusterRoleBinding: 'clusterrolebindings'
};

/** The served resource type of an RBAC kind, to open a binding or role. */
export function rbacResource(resources: ResourceInfo[], kind: string) {
	return resources.find((r) => r.group === 'rbac.authorization.k8s.io' && r.plural === PLURALS[kind]);
}

/** Whether any of a subject's grants is cluster-wide and allows everything. */
export function isAdmin(grants: Grant[]) {
	return grants.some(
		(g) =>
			g.namespace === null &&
			g.rules.some((r) => r.verbs.includes('*') && r.resources.includes('*') && r.apiGroups.includes('*'))
	);
}

/**
 * Subjects whose name, kind or namespace contains the text. `can VERB RESOURCE` (such as
 * `can delete pods` or `can create pods/exec`) keeps those whose rules allow that instead.
 */
export function filterSubjects(subjects: SubjectInfo[], text: string) {
	const t = text.trim().toLowerCase();
	if (!t) return subjects;
	const can = /^can\s+(\S+)\s+(\S+)$/.exec(t);
	if (can) {
		const [, verb, what] = can;
		const [resource, subresource = ''] = what.split('/');
		return subjects.filter((s) => s.grants.some((g) => g.rules.some((r) => ruleAllows(r, verb, null, resource, subresource))));
	}
	return subjects.filter((s) => [s.name, s.kind, s.namespace ?? ''].some((v) => v.toLowerCase().includes(t)));
}

/** Whether a rule allows `verb` on a resource; a null group matches any group. */
export function ruleAllows(rule: Rule, verb: string, group: string | null, resource: string, subresource = ''): boolean {
	const name = subresource ? `${resource}/${subresource}` : resource;
	const verbs = rule.verbs.includes('*') || rule.verbs.includes(verb);
	const groups = group === null || rule.apiGroups.includes('*') || rule.apiGroups.includes(group);
	const resources = rule.resources.some((r) => r === '*' || r === name || (!!subresource && (r === `${resource}/*` || r === `*/${subresource}`)));
	return verbs && groups && resources;
}

/** A name for a Role from what it allows, as Kubernetes accepts names. */
function roleName(check: AccessCheck): string {
	const parts = ['kubri', check.verb, check.resource, check.subresource ?? ''].filter(Boolean).join('-');
	return parts.toLowerCase().replace(/[^a-z0-9-]+/g, '-').replace(/-+/g, '-').slice(0, 63).replace(/-$/, '');
}

/** The subject a RoleBinding names for a user, or for a service account from its username. */
function subjectYaml(identity: Identity | null): string[] {
	const sa = identity && /^system:serviceaccount:([^:]+):([^:]+)$/.exec(identity.username);
	if (sa) return ['  - kind: ServiceAccount', `    name: ${sa[2]}`, `    namespace: ${sa[1]}`];
	return ['  - kind: User', `    name: ${quote(identity?.username ?? 'YOUR-USER-NAME')}`, '    apiGroup: rbac.authorization.k8s.io'];
}

function quote(text: string): string {
	return /^[A-Za-z0-9._@-]+$/.test(text) && !/^(true|false|null|yes|no|on|off|~)$/i.test(text) ? text : JSON.stringify(text);
}

/**
 * A Role and RoleBinding (ClusterRole and ClusterRoleBinding without a namespace) that would
 * allow `check`, to send to an admin.
 */
export function grantYaml(check: AccessCheck, identity: Identity | null): string {
	const name = roleName(check);
	const namespaced = !!check.namespace;
	const [role, binding] = namespaced ? ['Role', 'RoleBinding'] : ['ClusterRole', 'ClusterRoleBinding'];
	const meta = (kind: string) => [
		'apiVersion: rbac.authorization.k8s.io/v1',
		`kind: ${kind}`,
		'metadata:',
		`  name: ${name}`,
		...(namespaced ? [`  namespace: ${check.namespace}`] : [])
	];
	const resource = check.subresource ? `${check.resource}/${check.subresource}` : check.resource;
	return [
		...meta(role),
		'rules:',
		`  - apiGroups: [${JSON.stringify(check.group)}]`,
		`    resources: [${JSON.stringify(resource)}]`,
		// Rules cannot name objects for verbs that act on no existing object.
		...(check.name && !['create', 'list', 'watch', 'deletecollection'].includes(check.verb)
			? [`    resourceNames: [${JSON.stringify(check.name)}]`]
			: []),
		`    verbs: [${JSON.stringify(check.verb)}]`,
		'---',
		...meta(binding),
		'subjects:',
		...subjectYaml(identity),
		'roleRef:',
		'  apiGroup: rbac.authorization.k8s.io',
		`  kind: ${role}`,
		`  name: ${name}`,
		''
	].join('\n');
}
