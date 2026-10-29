import type { ContainerImage, DeleteOptions, DrainOptions, ObjectRef, ResourceRef } from './api';

/** What Kubri is about to do, in the terms `kubectl` needs. */
export type KubectlOp =
	| { op: 'describe' }
	| { op: 'yaml' }
	| { op: 'logs'; previous?: boolean; container?: string | null; workload?: boolean }
	| { op: 'decode' }
	| { op: 'shell'; container?: string | null }
	| { op: 'attach'; container?: string | null }
	| { op: 'portForward'; local?: number | null; remote?: number | null; address?: string | null }
	| { op: 'cordon' }
	| { op: 'uncordon' }
	| { op: 'drain'; options?: DrainOptions }
	| { op: 'trigger' }
	| { op: 'suspend'; suspend: boolean }
	| { op: 'edit' }
	| { op: 'scale'; replicas?: number | null }
	| { op: 'restart' }
	| { op: 'image'; images?: ContainerImage[] }
	| { op: 'labels'; labels?: Record<string, string | null>; annotations?: Record<string, string | null> }
	| { op: 'delete'; targets?: ObjectRef[]; options?: DeleteOptions }
	| { op: 'rollback'; deployment?: string | null; revision?: string | null };

/** Quotes a word for a POSIX shell only where it needs it. */
export function shellQuote(word: string): string {
	if (word !== '' && /^[A-Za-z0-9_./:=@%+,^-]+$/.test(word)) return word;
	return `'${word.replaceAll("'", `'\\''`)}'`;
}

/** The resource as kubectl names it unambiguously: `pods`, `deployments.apps`. */
export function kubectlResource(resource: Pick<ResourceRef, 'group' | 'plural'>): string {
	return resource.group ? `${resource.plural}.${resource.group}` : resource.plural;
}

function command(context: string | null | undefined, namespace: string | null | undefined, args: string[]): string {
	const scope = [...(context ? ['--context', context] : []), ...(namespace ? ['-n', namespace] : [])];
	return ['kubectl', ...scope, ...args].map(shellQuote).join(' ');
}

/** Keys to set as `key=value` and to remove as `key-`, as `kubectl label` takes them. */
function keyArgs(map: Record<string, string | null>): string[] {
	return Object.entries(map).map(([k, v]) => (v === null ? `${k}-` : `${k}=${v}`));
}

/**
 * The `kubectl` command that does what Kubri does for `op` on `target`, in `context`.
 * Values not known yet appear as placeholders in capitals, such as `REPLICAS`.
 */
export function kubectlCommand(context: string | null | undefined, target: ObjectRef, op: KubectlOp): string {
	const ns = target.namespace ?? null;
	const ref = `${kubectlResource(target.resource)}/${target.name}`;
	const run = (...args: string[]) => command(context, ns, args);
	const container = (c?: string | null) => (c ? ['-c', c] : []);
	switch (op.op) {
		case 'describe':
			return run('describe', ref);
		case 'yaml':
			return run('get', ref, '-o', 'yaml');
		case 'edit':
			return run('edit', ref);
		case 'logs':
			return run('logs', ref, ...container(op.container), ...(op.workload ? ['--all-pods', '--prefix'] : []), ...(op.previous ? ['--previous'] : ['-f']));
		case 'decode':
			return run('get', ref, '-o', 'go-template={{range $k, $v := .data}}{{$k}}={{$v | base64decode}}{{"\\n"}}{{end}}');
		case 'shell':
			return run('exec', '-it', ref, ...container(op.container), '--', 'sh', '-c', 'bash || sh');
		case 'attach':
			return run('attach', '-it', ref, ...container(op.container));
		case 'portForward': {
			const ports = `${op.local || ''}:${op.remote ?? 'PORT'}`;
			const address = op.address && op.address !== '127.0.0.1' && op.address !== 'localhost' ? ['--address', op.address] : [];
			return run('port-forward', ref, ports.startsWith(':') ? ports.slice(1) : ports, ...address);
		}
		case 'cordon':
		case 'uncordon':
			return command(context, null, [op.op, target.name]);
		case 'drain': {
			const o = op.options ?? {};
			return command(context, null, [
				'drain',
				target.name,
				...(o.ignoreDaemonsets ? ['--ignore-daemonsets'] : []),
				...(o.deleteEmptydirData ? ['--delete-emptydir-data'] : []),
				...(o.force ? ['--force'] : []),
				...(o.gracePeriodSeconds != null ? [`--grace-period=${o.gracePeriodSeconds}`] : [])
			]);
		}
		case 'trigger':
			return run('create', 'job', `--from=cronjob/${target.name}`, `${target.name}-manual`);
		case 'suspend':
			return run('patch', ref, '--type=merge', '-p', JSON.stringify({ spec: { suspend: op.suspend } }));
		case 'scale':
			return run('scale', ref, `--replicas=${op.replicas ?? 'REPLICAS'}`);
		case 'restart':
			return run('rollout', 'restart', ref);
		case 'image': {
			const images = op.images?.length ? op.images.map((i) => `${i.name}=${i.image}`) : ['CONTAINER=IMAGE'];
			return run('set', 'image', ref, ...images);
		}
		case 'labels': {
			const labels = keyArgs(op.labels ?? {});
			const annotations = keyArgs(op.annotations ?? {});
			const lines = [];
			if (labels.length || !annotations.length) lines.push(run('label', '--overwrite', ref, ...(labels.length ? labels : ['KEY=VALUE'])));
			if (annotations.length) lines.push(run('annotate', '--overwrite', ref, ...annotations));
			return lines.join('\n');
		}
		case 'delete': {
			const targets = op.targets?.length ? op.targets : [target];
			const o = op.options ?? {};
			const flags = [
				...(o.force ? ['--force'] : []),
				...(o.gracePeriodSeconds != null ? [`--grace-period=${o.gracePeriodSeconds}`] : []),
				...(o.propagation && o.propagation !== 'background' ? [`--cascade=${o.propagation}`] : [])
			];
			// One command per namespace, since kubectl takes one.
			const byNamespace = new Map<string | null, ObjectRef[]>();
			for (const t of targets) byNamespace.set(t.namespace ?? null, [...(byNamespace.get(t.namespace ?? null) ?? []), t]);
			return [...byNamespace]
				.map(([namespace, list]) =>
					command(context, namespace, ['delete', ...list.map((t) => `${kubectlResource(t.resource)}/${t.name}`), ...flags])
				)
				.join('\n');
		}
		case 'rollback':
			return run('rollout', 'undo', `deployments.apps/${op.deployment ?? 'DEPLOYMENT'}`, `--to-revision=${op.revision ?? 'REVISION'}`);
	}
}

/** The `helm` command for a rollback or an uninstall Kubri runs. */
export function helmCommand(
	context: string | null | undefined,
	release: { name: string; namespace: string },
	op: { op: 'rollback'; revision: number } | { op: 'uninstall' }
): string {
	const args =
		op.op === 'rollback' ? ['helm', 'rollback', release.name, String(op.revision)] : ['helm', 'uninstall', release.name];
	return [...args, '-n', release.namespace, ...(context ? ['--kube-context', context] : [])].map(shellQuote).join(' ');
}
