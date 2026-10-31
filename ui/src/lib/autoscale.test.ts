import { describe, expect, it } from 'vitest';
import { autoscalerOf } from './autoscale';

const hpa = (spec: Record<string, unknown>) => ({ metadata: { name: 'frontend' }, spec });

describe('autoscalerOf', () => {
	it('matches the workload it targets, with its limits', () => {
		const spec = { scaleTargetRef: { kind: 'Deployment', name: 'frontend' }, minReplicas: 3, maxReplicas: 6 };
		expect(autoscalerOf(hpa(spec), { kind: 'Deployment', name: 'frontend' })).toEqual({ name: 'frontend', min: 3, max: 6 });
	});
	it('ignores other workloads', () => {
		const spec = { scaleTargetRef: { kind: 'Deployment', name: 'cart' }, maxReplicas: 4 };
		expect(autoscalerOf(hpa(spec), { kind: 'Deployment', name: 'frontend' })).toBeNull();
		expect(autoscalerOf(hpa(spec), { kind: 'StatefulSet', name: 'cart' })).toBeNull();
	});
	it('defaults the minimum to one replica', () => {
		const spec = { scaleTargetRef: { kind: 'Deployment', name: 'frontend' }, maxReplicas: 4 };
		expect(autoscalerOf(hpa(spec), { kind: 'Deployment', name: 'frontend' })?.min).toBe(1);
	});
});
