import { t } from './i18n';

/** What to try when connecting to a context failed, from the error; null when there is nothing better to say than the error itself. */
export function connectHint(error: string): string | null {
	const text = error.toLowerCase();
	if (/certificate|x509|unknown ca|tls handshake|invalid peer/.test(text)) {
		return t("Kubri does not trust the cluster's certificate. Check the certificate authority in your kubeconfig, or get a fresh kubeconfig for this cluster.");
	}
	if (/\b401\b|unauthorized|expired|invalid_grant|token has expired|authentication required/.test(text)) {
		return t('The cluster turned your credentials down. Sign in again, or get fresh credentials, then try again.');
	}
	if (/\b403\b|forbidden/.test(text)) {
		return t('You are signed in, but may not read this cluster. Ask an admin for access, or choose another context.');
	}
	if (/executable .* not found|no such file or directory.*exec|failed to run|exec plugin|getting credentials/.test(text)) {
		return t('This context signs in by running a command, which failed. Check that the command is installed and works in a terminal.');
	}
	if (/connection refused|timed out|timeout|no route to host|network is unreachable|dns|failed to lookup|name or service not known|connection reset|host unreachable/.test(text)) {
		return t("The cluster's server does not answer. Check that the cluster is running and that you are on its network or VPN.");
	}
	return null;
}

type Failure = 'certificate' | 'credentials' | 'access' | 'command' | 'unreachable' | 'other';

/** Why connecting failed, by the error's words. */
function failure(error: string): Failure {
	const text = error.toLowerCase();
	if (/certificate|x509|unknown ca|tls handshake|invalid peer/.test(text)) return 'certificate';
	if (/\b401\b|unauthorized|expired|invalid_grant|token has expired|authentication required/.test(text)) return 'credentials';
	if (/\b403\b|forbidden/.test(text)) return 'access';
	if (/executable .* not found|no such file or directory.*exec|failed to run|exec plugin|getting credentials/.test(text)) return 'command';
	if (/connection refused|timed out|timeout|no route to host|network is unreachable|dns|failed to lookup|name or service not known|connection reset|host unreachable/.test(text)) {
		return 'unreachable';
	}
	return 'other';
}

/** A few words for why connecting failed, for lists such as the context switcher. */
export function connectSummary(error: string): string {
	return {
		certificate: t('Certificate not trusted'),
		credentials: t('Sign-in failed'),
		access: t('No access'),
		command: t('Sign-in command failed'),
		unreachable: t('Unreachable'),
		other: t('Could not connect')
	}[failure(error)];
}

/** Whether connecting failed because the server did not answer, rather than turning Kubri down. */
export function noAnswer(error: string): boolean {
	return failure(error) === 'unreachable';
}
