import { describe, expect, it } from 'vitest';
import { canSuspendFlux, gitopsStatus, managerNote } from './gitops';

const kustomizations = { group: 'kustomize.toolkit.fluxcd.io', version: 'v1', plural: 'kustomizations' };
const applications = { group: 'argoproj.io', version: 'v1alpha1', plural: 'applications' };

describe('gitopsStatus', () => {
	it('reads a Kustomization and its inventory', () => {
		const status = gitopsStatus(kustomizations, {
			spec: { interval: '10m', path: './apps', sourceRef: { kind: 'GitRepository', name: 'shop' }, suspend: true },
			status: {
				lastAppliedRevision: 'main@sha1:4f2c',
				conditions: [{ type: 'Ready', status: 'True', reason: 'ReconciliationSucceeded', lastTransitionTime: '2026-10-29T20:00:00Z' }],
				inventory: { entries: [{ id: 'shop_web_apps_Deployment', v: 'v1' }, { id: '_shop__Namespace', v: 'v1' }] }
			}
		})!;
		expect(status.state).toBe('Suspended · Ready');
		expect(status.health).toBe('warning');
		expect(status.facts.map((f) => f.label)).toEqual(['Revision', 'Source', 'Path', 'Interval', 'Last reconcile']);
		expect(status.managed).toEqual([
			{ group: 'apps', version: 'v1', kind: 'Deployment', namespace: 'shop', name: 'web' },
			{ group: '', version: 'v1', kind: 'Namespace', namespace: null, name: 'shop' }
		]);
	});

	it('says why a HelmRelease failed', () => {
		const status = gitopsStatus(
			{ group: 'helm.toolkit.fluxcd.io', version: 'v2', plural: 'helmreleases' },
			{ status: { conditions: [{ type: 'Ready', status: 'False', reason: 'ArtifactFailed', message: 'no chart' }] } }
		)!;
		expect([status.state, status.health, status.message]).toEqual(['Failed: ArtifactFailed', 'failed', 'no chart']);
	});

	it('reads an Application', () => {
		const status = gitopsStatus(applications, {
			spec: {
				source: { repoURL: 'https://git/x', path: 'deploy', targetRevision: 'main' },
				destination: { server: 'https://kubernetes.default.svc', namespace: 'shop' },
				syncPolicy: { automated: { selfHeal: true, prune: true } }
			},
			status: {
				sync: { status: 'OutOfSync', revision: 'abc' },
				health: { status: 'Degraded' },
				operationState: { phase: 'Failed', message: 'one failed' },
				resources: [{ group: 'apps', version: 'v1', kind: 'Deployment', namespace: 'shop', name: 'worker', status: 'OutOfSync', health: { status: 'Healthy' } }]
			}
		})!;
		expect(status.state).toBe('OutOfSync · Degraded · sync failed');
		expect(status.health).toBe('failed');
		expect(status.message).toBe('one failed');
		expect(status.facts.find((f) => f.label === 'Auto-sync')?.value).toBe('On (self-heal, prune)');
		expect(status.managed[0]).toMatchObject({ kind: 'Deployment', sync: 'OutOfSync', health: 'Healthy' });
	});

	it('leaves other kinds alone', () => {
		expect(gitopsStatus({ group: 'apps', version: 'v1', plural: 'deployments' }, {})).toBeNull();
		expect(canSuspendFlux(kustomizations)).toBe(true);
		expect(canSuspendFlux({ group: 'image.toolkit.fluxcd.io', version: 'v1', plural: 'imagepolicies' })).toBe(false);
	});
});

describe('managerNote', () => {
	const owner = { resource: applications, namespace: 'argocd', name: 'payments' };
	it('says whether changes stay', () => {
		expect(managerNote({ tool: 'Argo CD', owner, kind: 'Application', reverts: true, paused: false })).toContain('right away');
		expect(managerNote({ tool: 'Flux', owner, kind: 'Kustomization', reverts: false, paused: true })).toContain('suspended');
		expect(managerNote({ tool: 'Flux', owner, kind: 'HelmRelease', reverts: false, paused: false })).toContain('next upgrade');
	});
});
