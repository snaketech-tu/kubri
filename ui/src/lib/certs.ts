import type { Health, ResourceRef } from './api';
import type { ToolStatus } from './gitops';
import { plural, t } from './i18n';

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

/** Days, or hours below two days: how many, and whether they are days. */
function span(ms: number): [number, boolean] {
	const hours = Math.floor(Math.abs(ms) / 3_600_000);
	return hours < 48 ? [hours, false] : [Math.floor(hours / 24), true];
}

function expiredAgo(ms: number) {
	const [n, days] = span(ms);
	return days ? plural(n, 'Expired {n} day ago', 'Expired {n} days ago') : plural(n, 'Expired {n} hour ago', 'Expired {n} hours ago');
}

function expiresIn(ms: number) {
	const [n, days] = span(ms);
	return days ? plural(n, 'Expires in {n} day', 'Expires in {n} days') : plural(n, 'Expires in {n} hour', 'Expires in {n} hours');
}

function readyFor(ms: number) {
	const [n, days] = span(ms);
	return days
		? plural(n, 'Ready, expires in {n} day', 'Ready, expires in {n} days')
		: plural(n, 'Ready, expires in {n} hour', 'Ready, expires in {n} hours');
}

/**
 * Whether a Certificate is ready and far from expiry, as the overview and the sanitizer judge
 * it: close to expiry, past where cert-manager should have renewed it, is a warning.
 */
export function certificateHealth(cert: Cert, now: number): { health: Health; text: string } {
	const ready = cert.status?.conditions?.find((c) => c.type === 'Ready');
	const end = cert.status?.notAfter ? Date.parse(cert.status.notAfter) : NaN;
	if (end <= now) return { health: 'failed', text: expiredAgo(now - end) };
	if (ready?.status === 'False') return { health: 'failed', text: t('Not ready: {reason}', { reason: ready.message || ready.reason || t('unknown') }) };
	if (!ready) return { health: 'warning', text: t('Not issued yet') };
	if (Number.isNaN(end)) return { health: 'ok', text: t('Ready') };
	const left = end - now;
	const start = cert.status?.notBefore ? Date.parse(cert.status.notBefore) : NaN;
	const part = (divisor: number, capDays: number) => Math.min(capDays * DAY, Number.isNaN(start) ? Infinity : (end - start) / divisor);
	if (left < part(6, 7)) return { health: 'failed', text: expiresIn(left) };
	if (left < part(3, 21)) return { health: 'warning', text: expiresIn(left) };
	return { health: 'ok', text: readyFor(left) };
}

export function certificateStatus(resource: ResourceRef, object: unknown, now: number): ToolStatus | null {
	if (!isCertificates(resource)) return null;
	const cert = object as Cert;
	const { health, text } = certificateHealth(cert, now);
	const ready = cert.status?.conditions?.find((c) => c.type === 'Ready');
	const issuing = cert.status?.conditions?.find((c) => c.type === 'Issuing' && c.status === 'True');
	const facts: ToolStatus['facts'] = [];
	const add = (label: string, value: string | null | undefined, time = false) => value && facts.push({ label, value, time });
	add(t('Expires'), cert.status?.notAfter, true);
	add(t('Renews'), cert.status?.renewalTime, true);
	add(t('Issuer'), cert.spec?.issuerRef ? `${cert.spec.issuerRef.kind ?? 'Issuer'} ${cert.spec.issuerRef.name}` : null);
	add(t('Secret'), cert.spec?.secretName);
	add(t('Names'), [cert.spec?.commonName, ...(cert.spec?.dnsNames ?? [])].filter((n, i, all) => n && all.indexOf(n) === i).join(', '));
	add(t('Lifetime'), cert.spec?.duration);
	return {
		state: issuing ? t('{state} · issuing', { state: text }) : text,
		health,
		message: issuing ? (issuing.message ?? null) : ready?.status === 'True' ? null : (ready?.message ?? null),
		facts,
		managed: []
	};
}
