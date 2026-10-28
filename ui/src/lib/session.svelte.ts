import { isTauri } from '@tauri-apps/api/core';

/** Whether kubri-server asks for a password, and whether this browser has signed in. */
interface SessionState {
	required: boolean;
	signedIn: boolean;
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

	async check() {
		if (isTauri()) return;
		try {
			const response = await fetch('/api/session');
			if (response.ok) {
				const state: SessionState = await response.json();
				this.required = state.required;
				this.signedIn = state.signedIn;
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
			throw err instanceof TypeError ? new Error('Could not reach kubri-server') : err;
		}
		this.signedIn = true;
	}

	async signOut() {
		await post('/logout').catch(() => {});
		this.signedIn = false;
	}

	/** The server turned a request away: the session ended, or the server restarted. */
	expired() {
		this.required = true;
		this.signedIn = false;
	}
}

export const session = new Session();
