import { backend, type KubeObject, type ObjectRef, type ResourceRef } from './api';

/** A HorizontalPodAutoscaler that sets the replicas of a workload. */
export interface Autoscaler {
	name: string;
	min: number;
	max: number;
}

const SCALABLE = ['Deployment', 'StatefulSet', 'ReplicaSet'];

/** Whether the autoscaler object targets the workload, and with which limits. */
export function autoscalerOf(hpa: KubeObject, workload: { kind: string; name: string }): Autoscaler | null {
	const spec = hpa.spec as { scaleTargetRef?: { kind?: string; name?: string }; minReplicas?: number; maxReplicas?: number } | undefined;
	const target = spec?.scaleTargetRef;
	if (!target || target.kind !== workload.kind || target.name !== workload.name) return null;
	return { name: hpa.metadata.name, min: spec?.minReplicas ?? 1, max: spec?.maxReplicas ?? 0 };
}

/** The autoscaler that manages the workload's replicas, if any. The search lists them; a failure means none. */
export async function autoscalerFor(
	target: ObjectRef,
	kind: string,
	autoscalers: ResourceRef | undefined
): Promise<Autoscaler | null> {
	if (!autoscalers || !SCALABLE.includes(kind)) return null;
	try {
		const found = await backend.search({
			text: '',
			kinds: [autoscalers],
			allKinds: false,
			namespace: target.namespace,
			labels: '',
			fields: [],
			contexts: [],
			limit: 100
		});
		for (const hit of found.hits) {
			const hpa = await backend.getObject(hit.object);
			const match = autoscalerOf(hpa, { kind, name: target.name });
			if (match) return match;
		}
	} catch {
		// No access to autoscalers, or none served: nothing to warn about.
	}
	return null;
}
