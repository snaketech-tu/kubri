import { SvelteMap } from 'svelte/reactivity';
import { backend, type AccessCheck, type ResourceRef } from './api';

/** Checks asked together, after the UI asked for them in one go. */
const BATCH_DELAY = 30;

function keyOf(check: AccessCheck): string {
	return [check.verb, check.group, check.resource, check.subresource ?? '', check.namespace ?? '', check.name ?? ''].join('|');
}

/** A check for `verb` on a resource, or on one of its objects or subresources. */
export function need(
	verb: string,
	resource: Pick<ResourceRef, 'group' | 'plural'>,
	namespace: string | null | undefined,
	more: { subresource?: string; name?: string | null } = {}
): AccessCheck {
	return {
		verb,
		group: resource.group,
		resource: resource.plural,
		subresource: more.subresource ?? '',
		namespace: namespace ?? null,
		name: more.name ?? null
	};
}

/** Such as "`delete` on `pods` in `shop`", worded like the core's messages. */
export function describeCheck(check: AccessCheck): string {
	let resource = check.group ? `${check.resource}.${check.group}` : check.resource;
	if (check.subresource) resource += `/${check.subresource}`;
	return check.namespace ? `\`${check.verb}\` on \`${resource}\` in \`${check.namespace}\`` : `\`${check.verb}\` on \`${resource}\``;
}

/**
 * What the current user may do in the connected context, asked of the core in batches and
 * remembered until the context changes or `refresh` is called.
 */
class Access {
	#answers = new SvelteMap<string, boolean | null>();
	#queued = new Map<string, AccessCheck>();
	#asking = new Set<string>();
	#timer: ReturnType<typeof setTimeout> | null = null;
	#context: string | null = null;

	/** Starts over for another context. */
	reset(context: string | null) {
		if (context === this.#context) return;
		this.#context = context;
		this.#queued.clear();
		this.#asking.clear();
		// After the current update: this may run while the UI derives what to show.
		queueMicrotask(() => this.#answers.clear());
	}

	/** True or false once known; undefined while asking, null where the cluster cannot say. */
	allowed(check: AccessCheck): boolean | null | undefined {
		const key = keyOf(check);
		const answer = this.#answers.get(key);
		if (answer !== undefined) return answer;
		if (!this.#asking.has(key) && !this.#queued.has(key)) {
			this.#queued.set(key, check);
			this.#timer ??= setTimeout(() => this.#flush(), BATCH_DELAY);
		}
		return undefined;
	}

	/** Why `check` is refused, or null while it is allowed or not known to be refused. */
	denial(check: AccessCheck): string | null {
		return this.allowed(check) === false ? `Needs ${describeCheck(check).replaceAll('`', '')}` : null;
	}

	/** Asks again for everything, as after a role may have changed. */
	async refresh() {
		const checks = [...this.#answers.keys()].map((key) => {
			const [verb, group, resource, subresource, namespace, name] = key.split('|');
			return { verb, group, resource, subresource, namespace: namespace || null, name: name || null };
		});
		this.#answers.clear();
		if (!checks.length) return;
		await this.#ask(checks, true);
	}

	async #flush() {
		this.#timer = null;
		const checks = [...this.#queued.values()];
		this.#queued.clear();
		// The core answers at most 200 at once.
		for (let i = 0; i < checks.length; i += 200) await this.#ask(checks.slice(i, i + 200), false);
	}

	async #ask(checks: AccessCheck[], fresh: boolean) {
		const context = this.#context;
		const keys = checks.map(keyOf);
		for (const key of keys) this.#asking.add(key);
		try {
			const answers = await backend.access(checks, fresh);
			if (context !== this.#context) return;
			keys.forEach((key, i) => this.#answers.set(key, answers[i] ?? null));
		} catch {
			// Not known: actions stay offered, and the API server decides.
			if (context === this.#context) for (const key of keys) this.#answers.set(key, null);
		} finally {
			for (const key of keys) this.#asking.delete(key);
		}
	}
}

export const access = new Access();
