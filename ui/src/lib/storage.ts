/** Reads a choice kept in the browser; null without storage. */
export function stored(key: string): string | null {
	try {
		return localStorage.getItem(key);
	} catch {
		return null;
	}
}

/**
 * Keeps a choice in the browser; null forgets it. Without storage it lasts until the page
 * reloads, and the answer is false.
 */
export function store(key: string, value: string | null): boolean {
	try {
		if (value === null) localStorage.removeItem(key);
		else localStorage.setItem(key, value);
		return true;
	} catch {
		return false;
	}
}

/** A list kept in the browser as JSON; empty without one. */
export function storedList<T>(key: string): T[] {
	try {
		const parsed = JSON.parse(stored(key) ?? '[]');
		return Array.isArray(parsed) ? parsed : [];
	} catch {
		return [];
	}
}
