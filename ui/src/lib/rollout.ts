import type { KubeObject } from './api';

export interface Revision {
	revision: number;
	/** The ReplicaSet or ControllerRevision that holds this revision. */
	name: string;
	/** `name:tag` of each container of the pod template. */
	images: string[];
	/** Pods of this revision; null where the revision does not say, as for StatefulSets. */
	replicas: number | null;
	created: string | null;
	/** The revision the Deployment runs now. */
	current: boolean;
	/** What the change cause annotation says, when someone set it. */
	cause: string | null;
}

const REVISION = 'deployment.kubernetes.io/revision';

/** The revisions of a Deployment from its ReplicaSets, newest first. */
export function revisionsOf(deployment: KubeObject, replicaSets: KubeObject[]): Revision[] {
	const currentRevision = Number(deployment.metadata.annotations?.[REVISION]);
	const out: Revision[] = [];
	for (const rs of replicaSets) {
		const revision = Number(rs.metadata.annotations?.[REVISION]);
		if (!Number.isFinite(revision) || revision <= 0) continue;
		const containers: any[] = rs.spec?.template?.spec?.containers ?? [];
		out.push({
			revision,
			name: rs.metadata.name,
			images: containers.map((c) => String(c.image ?? '')),
			replicas: Number(rs.status?.replicas ?? 0),
			created: rs.metadata.creationTimestamp ?? null,
			current: revision === currentRevision,
			cause: rs.metadata.annotations?.['kubernetes.io/change-cause'] ?? null
		});
	}
	return out.sort((a, b) => b.revision - a.revision);
}

/**
 * The revisions of a StatefulSet or DaemonSet from its ControllerRevisions, newest first. A
 * StatefulSet names the one it runs in `status.updateRevision`; a DaemonSet runs its newest.
 */
export function controllerRevisionsOf(owner: KubeObject, revisions: KubeObject[]): Revision[] {
	const out: Revision[] = [];
	for (const cr of revisions as any[]) {
		const revision = Number(cr.revision);
		if (!Number.isFinite(revision) || revision <= 0) continue;
		const containers: any[] = cr.data?.spec?.template?.spec?.containers ?? [];
		out.push({
			revision,
			name: cr.metadata.name,
			images: containers.map((c) => String(c.image ?? '')),
			replicas: null,
			created: cr.metadata.creationTimestamp ?? null,
			current: false,
			cause: cr.metadata.annotations?.['kubernetes.io/change-cause'] ?? null
		});
	}
	out.sort((a, b) => b.revision - a.revision);
	const running = (owner as any).status?.updateRevision;
	const current = running ? out.find((r) => r.name === running) : out[0];
	if (current) current.current = true;
	return out;
}

/** The images that differ from another revision, as `from → to`; empty when they are the same. */
export function imageChanges(from: string[], to: string[]): string[] {
	const out: string[] = [];
	const length = Math.max(from.length, to.length);
	for (let i = 0; i < length; i++) {
		if ((from[i] ?? '') !== (to[i] ?? '')) out.push(`${from[i] ?? '—'} → ${to[i] ?? '—'}`);
	}
	return out;
}
