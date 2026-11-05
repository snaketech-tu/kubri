/**
 * Texts that come from Kubri's core in English. The core's error messages are known by their
 * format in `crates/kubri-core/src/error.rs`, which works as their code: a test checks that the
 * two lists agree, so a changed message cannot slip by. Values in them, such as a name, stay as
 * they are; a message that matches no format stays English.
 */
import { t } from '.';

/** The formats of the core's errors, `{0}` for the value. Not the ones that are only a value. */
export const ERROR_FORMATS = [
	'no kubeconfig loaded: {0}',
	'context `{0}` does not exist in the kubeconfig',
	'not connected to a cluster, select a context first',
	'resource `{0}` is not served by the cluster',
	'{0} does not exist',
	'invalid object: {0}',
	'invalid kubeconfig: {0}',
	'failed to read kubeconfig: {0}',
	'the target belongs to context `{0}`, not the current one; review it again',
	'timed out while connecting to the cluster',
	'connecting to {0} was cancelled',
	'Kubri is read-only ({0}), so it does not change the cluster',
	'not allowed: needs {0}',
	'Explain: {0}',
	// From kube_message, for a 401 of the API server.
	'the cluster did not accept the credentials in the kubeconfig ({0})'
];

/** Whole texts the core sends, such as why a context is read-only. */
export const CORE_TEXTS = [
	'this is a snapshot: it shows what was saved and cannot change anything',
	'it was started with --read-only or KUBRI_READ_ONLY',
	'readOnly is set in the k9s configuration',
	'this context is set read-only in Kubri',
	'AI agents never change a production context',
	"AI agents may not change this context; allow it in Kubri's settings",
	'this context is locked; AI agents cannot unlock it',
	'this context is locked; unlock it to make changes',
	'A snapshot does not change, so there is nothing to record',
	'This Kubri does not record changes',
	'Recording is turned off in the settings',
	'Kubri records only the context it is connected to'
];

const escape = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

const PATTERNS = ERROR_FORMATS.map((format) => ({
	format,
	pattern: new RegExp(`^${escape(format).replace('\\{0\\}', '([\\s\\S]+?)')}$`)
}));

/** A text from the core in the UI's language, where Kubri knows it. */
export function coreText(text: string): string {
	if (CORE_TEXTS.includes(text)) return t(text);
	for (const { format, pattern } of PATTERNS) {
		const match = text.match(pattern);
		if (match) return t(format, match[1] === undefined ? undefined : { 0: coreText(match[1]) });
	}
	return text;
}
