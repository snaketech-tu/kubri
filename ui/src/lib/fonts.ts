/** Generic CSS families, which must not be quoted. */
const GENERIC = new Set(['monospace', 'ui-monospace', 'serif', 'sans-serif', 'system-ui', 'cursive', 'fantasy']);

/**
 * The fonts the user named, separated by commas, in front of Kubri's own. Names are quoted
 * so that ones like `Source Code Pro 2` work; quoted names and generic families stay as
 * they are.
 */
export function fontStack(custom: string | undefined, base: string): string {
	const names = (custom ?? '')
		.split(',')
		.map((name) => name.trim())
		.filter(Boolean)
		.map((name) => (/^(['"]).*\1$/.test(name) || GENERIC.has(name) ? name : `"${name.replace(/"/g, '')}"`));
	return [...names, base.trim() || 'monospace'].join(', ');
}
