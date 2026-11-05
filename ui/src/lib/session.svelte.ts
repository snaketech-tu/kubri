import { isTauri } from '@tauri-apps/api/core';
import { t } from './i18n';

/** Someone signed in to a Kubri shared by a team, as the clusters know them. */
export interface Person {
	name: string;
	groups: string[];
}

/** Whether kubri-server asks people to sign in and how, and whether this browser has. */
interface SessionState {
	required: boolean;
	signedIn: boolean;
	method?: 'password' | 'oidc';
	/** The OpenID Connect provider's name, for the sign-in button. */
	provider?: string;
	person?: Person;
}

async function post(path: string, body?: unknown) {
	const response = await fetch(`/api${path}`, {
		method: 'POST',
		headers: body === undefined ? undefined : { 'content-type': 'application/json' },
		body: body === undefined ? undefined : JSON.stringify(body)
	});
	if (!response.ok) {
		const payload = await response.json().catch(() => null);
		throw new Error(payload?.error ?? `${response.status} ${response.statusText}`);
	}
}

/** Signing in to kubri-server. The desktop app has nothing to sign in to. */
class Session {
	checked = $state(isTauri());
	required = $state(false);
	signedIn = $state(true);
	method = $state<'password' | 'oidc' | null>(null);
	provider = $state<string | null>(null);
	/** Who signed in, on a Kubri shared by a team. */
	person = $state<Person | null>(null);

	async check() {
		if (isTauri()) return;
		try {
			const response = await fetch('/api/session');
			if (response.ok) {
				const state: SessionState = await response.json();
				this.required = state.required;
				this.signedIn = state.signedIn;
				this.method = state.method ?? null;
				this.provider = state.provider ?? null;
				this.person = state.person ?? null;
			}
		} catch {
			// The app shows that the server cannot be reached.
		}
		this.checked = true;
	}

	async signIn(password: string) {
		try {
			await post('/login', { password });
		} catch (err) {
			throw err instanceof TypeError ? new Error(t('Could not reach kubri-server')) : err;
		}
		this.signedIn = true;
	}

	async signOut() {
		await post('/logout').catch(() => {});
		this.signedIn = false;
		this.person = null;
	}

	#rechecking: Promise<void> | null = null;

	/**
	 * The server turned a request away: the session ended, or the server restarted. Asks the
	 * server instead of assuming it, once for a burst of failed requests.
	 */
	expired() {
		if (!this.signedIn) return;
		this.#rechecking ??= this.check().finally(() => (this.#rechecking = null));
	}
}

export const session = new Session();
