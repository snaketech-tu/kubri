import type { Health, ResourceRef } from './api';
import type { ToolStatus } from './gitops';

export const isCertificates = (resource: ResourceRef) => resource.group === 'cert-manager.io' && resource.plural === 'certificates';

const DAY = 86_400_000;

interface Cert {
	spec?: {
		secretName?: string;
		dnsNames?: string[];
		commonName?: string;
		duration?: string;
		renewBefore?: string;
		issuerRef?: { kind?: string; name?: string };
	};
	status?: {
		notBefore?: string;
		notAfter?: string;
		renewalTime?: string;
		conditions?: { type: string; status: string; reason?: string; message?: string }[];
	};
}

/** Days, or hours below two days. */
function span(ms: number) {
	const hours = Math.floor(Math.abs(ms) / 3_600_000);
	return hours < 48 ? `${hours} ${hours === 1 ? 'hour' : 'hours'}` : `${Math.floor(hours / 24)} days`;
}

/**
 * Whether a Certificate is ready and far from expiry, as the overview and the sanitizer judge
 * it: close to expiry, past where cert-manager should have renewed it, is a warning.
 */
export function certificateHealth(cert: Cert, now: number): { health: Health; text: string } {
	const ready = cert.status?.conditions?.find((c) => c.type === 'Ready');
	const end = cert.status?.notAfter ? Date.parse(cert.status.notAfter) : NaN;
	if (end <= now) return { health: 'failed', text: `Expired ${span(now - end)} ago` };
	if (ready?.status === 'False') return { health: 'failed', text: `Not ready: ${ready.message || ready.reason || 'unknown'}` };
	if (!ready) return { health: 'warning', text: 'Not issued yet' };
	if (Number.isNaN(end)) return { health: 'ok', text: 'Ready' };
	const left = end - now;
	const start = cert.status?.notBefore ? Date.parse(cert.status.notBefore) : NaN;
	const part = (divisor: number, capDays: number) => Math.min(capDays * DAY, Number.isNaN(start) ? Infinity : (end - start) / divisor);
	if (left < part(6, 7)) return { health: 'failed', text: `Expires in ${span(left)}` };
	if (left < part(3, 21)) return { health: 'warning', text: `Expires in ${span(left)}` };
	return { health: 'ok', text: `Ready, expires in ${span(left)}` };
}

export function certificateStatus(resource: ResourceRef, object: unknown, now: number): ToolStatus | null {
	if (!isCertificates(resource)) return null;
	const cert = object as Cert;
	const { health, text } = certificateHealth(cert, now);
	const ready = cert.status?.conditions?.find((c) => c.type === 'Ready');
	const issuing = cert.status?.conditions?.find((c) => c.type === 'Issuing' && c.status === 'True');
	const facts: ToolStatus['facts'] = [];
	const add = (label: string, value: string | null | undefined, time = false) => value && facts.push({ label, value, time });
	add('Expires', cert.status?.notAfter, true);
	add('Renews', cert.status?.renewalTime, true);
	add('Issuer', cert.spec?.issuerRef ? `${cert.spec.issuerRef.kind ?? 'Issuer'} ${cert.spec.issuerRef.name}` : null);
	add('Secret', cert.spec?.secretName);
	add('Names', [cert.spec?.commonName, ...(cert.spec?.dnsNames ?? [])].filter((n, i, all) => n && all.indexOf(n) === i).join(', '));
	add('Lifetime', cert.spec?.duration);
	return {
		state: issuing ? `${text} · issuing` : text,
		health,
		message: issuing ? (issuing.message ?? null) : ready?.status === 'True' ? null : (ready?.message ?? null),
		facts,
		managed: []
	};
}
