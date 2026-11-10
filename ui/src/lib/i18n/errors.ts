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

/**
 * How the core names what a check needs, as `describe` in `crates/kubri-core/src/access.rs` does:
 * "`delete` on `pods` in `shop`". The UI words its own checks the same way.
 */
export const CHECK_FORMATS = ['`{0}` on `{1}` in `{2}`', '`{0}` on `{1}`'];

/** What the core says above a list it shows only in part, in `crates/kubri-core/src/table.rs`. */
export const NOTICE_FORMATS = [
	"Showing the namespaces this context's settings name: {0}",
	'You may not list {0} in all namespaces; showing {1}',
	'{0} and {1} more'
];

/** Whole texts the core sends, such as why a context is read-only. */
export const CORE_TEXTS = [
	'this is a snapshot: it shows what was saved and cannot change anything',
	'it was started with --read-only or KUBRI_READ_ONLY',
	'readOnly is set in the k9s configuration',
	'this context is set read-only in Kubri',
	'AI agents never change a production context',
	'AI agents never change a production namespace',
	"AI agents may not change this context; allow it in Kubri's settings",
	'this context is locked; AI agents cannot unlock it',
	'this context is locked; unlock it to make changes',
	'A snapshot does not change, so there is nothing to record',
	'This Kubri does not record changes',
	'Recording is turned off in the settings',
	'Kubri records only the context it is connected to'
];

/**
 * Texts of the core's charts, local terminal, right-sizing and node checks, with the file each
 * comes from; `{0}` and `{1}` for values. A test checks that each still appears there.
 */
export const SOURCE_TEXTS: Record<string, string[]> = {
	'prometheus.rs': ['CPU', 'Memory', 'Network', 'Container restarts', 'Requests', 'Received', 'Sent', 'Request', 'Limit'],
	'integrations.rs': ['{0} restarted ({1})', 'Rollout', 'Revision {0}'],
	'local_terminal.rs': [
		'The local terminal is off here: it would be a shell on the server. Start the server with KUBRI_LOCAL_TERMINAL=true to allow it.',
		'AI agents cannot open a local terminal.',
		'Kubri is not connected to a context.',
		'A snapshot has no cluster for kubectl to talk to.',
		'changes are off for this context',
		'{0}. A local terminal would get around that, so it stays closed.',
		"kubectl {0} is more than one minor version away from the cluster's {1}; some commands may not work"
	],
	'rightsizing.rs': ['Prometheus at {0}, last 7 days', 'metrics-server, now'],
	// Why a pod does not fit on a node. The selector's "no label" first, as the other would match it too.
	'fixes.rs': [
		'the pod is bound to node {0}',
		'the node is not ready',
		'the node is cordoned',
		'taint {0}:{1} is not tolerated',
		'node selector wants {0}, the node has no {1} label',
		'node selector wants {0}, the node has {1}',
		'required node affinity is not met: {0}',
		'not enough CPU: the pod requests {0}, {1} is free',
		'not enough memory: the pod requests {0}, {1} is free',
		'the node runs as many pods as it may'
	],
	// What the checks find. Health texts such as "0/1 ready" stay as the lists show them.
	'sanitizer.rs': [
		'Scaled to 0',
		'Suspended',
		'Not used by any workload',
		'Not used by any workload, service account or ingress',
		'Not mounted by any pod',
		'Not bound by any role binding',
		'Not bound by any role binding or cluster role binding',
		'Not claimed by any volume claim',
		'Restarted {0} times',
		'Not managed by a controller, so nothing recreates it',
		'Uses the default service account',
		'No liveness or readiness probe',
		'No readiness probe',
		'No liveness probe',
		'No CPU or memory requests',
		'No memory request',
		'No CPU request',
		'No memory limit',
		'Has a port without a name',
		'May run as root',
		'Image {0} has no tag, so it means latest',
		'Image {0} uses the latest tag',
		'Type NodePort opens a port on every node',
		'Waiting for a load balancer address',
		'Selects no pods',
		'None of its pods is ready',
		'Port {0} targets {1}, which none of its pods names',
		'Scales {0} {1}, which does not exist',
		'Allows no disruptions right now, so node drains stop at its pods',
		'Sends traffic to service {0}, which does not exist',
		'Service {0} has no port {1}',
		'Its TLS secret {0} does not exist',
		'Refers to {0} {1}, which does not exist',
		'Binds service account {0}, which does not exist',
		'Cordoned, so no new pods are scheduled',
		'{0} use is at {1}% of what is allocatable'
	]
};

const SOURCE = Object.values(SOURCE_TEXTS).flat();
const WHOLE = [...CORE_TEXTS, ...SOURCE.filter((text) => !/\{\d\}/.test(text))];

const escape = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

// The texts of single files first, as "{0} does not exist" would match "Its TLS secret {0} does
// not exist" too; and the longer of the check formats first, for the same reason.
const PATTERNS = [...SOURCE.filter((text) => /\{\d\}/.test(text)), ...ERROR_FORMATS, ...CHECK_FORMATS, ...NOTICE_FORMATS].map((format) => ({
	format,
	pattern: new RegExp(`^${escape(format).replace(/\\\{\d\\\}/g, '([\\s\\S]+?)')}$`)
}));

/** A text from the core in the UI's language, where Kubri knows it. */
export function coreText(text: string): string {
	if (WHOLE.includes(text)) return t(text);
	// A reason at the start of a sentence, written with a capital.
	const small = text.charAt(0).toLowerCase() + text.slice(1);
	if (small !== text && WHOLE.includes(small)) {
		const translated = t(small);
		return translated.charAt(0).toUpperCase() + translated.slice(1);
	}
	for (const { format, pattern } of PATTERNS) {
		const match = text.match(pattern);
		if (match) return t(format, Object.fromEntries(match.slice(1).map((value, i) => [i, coreText(value)])));
	}
	return text;
}
