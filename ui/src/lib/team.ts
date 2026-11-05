/** The changes people made through a Kubri shared by a team, from its audit log. */
export interface AuditEntry {
	/** RFC 3339, in UTC. */
	time: string;
	person?: string;
	groups?: string[];
	client?: string;
	context?: string;
	action: string;
	target?: { resource?: string; namespace?: string; name?: string };
	detail?: Record<string, unknown>;
	ok: boolean;
	error?: string;
}

export interface TeamActivity {
	/** Whether these are everyone's changes, or only your own. */
	everyone: boolean;
	entries: AuditEntry[];
}

export async function teamActivity(): Promise<TeamActivity> {
	const response = await fetch('/api/activity');
	if (!response.ok) {
		const payload = await response.json().catch(() => null);
		throw new Error(payload?.error ?? `${response.status} ${response.statusText}`);
	}
	return response.json();
}

/** One line for a change: `scale deployments.apps shop/web · replicas 3`. */
export function describeChange(entry: AuditEntry): string {
	const target = entry.target;
	const object = target?.name ? (target.namespace ? `${target.namespace}/${target.name}` : target.name) : '';
	const what = [entry.action, target?.resource, object].filter(Boolean).join(' ');
	const details = Object.entries(entry.detail ?? {}).map(([key, value]) => `${words(key)} ${String(value)}`);
	return details.length ? `${what} · ${details.join(', ')}` : what;
}

/** `remotePort` → `remote port`. */
function words(key: string): string {
	return key.replace(/[A-Z]/g, (c) => ` ${c.toLowerCase()}`);
}
