import { backend, type ObjectRef } from './api';

/** Words that make a log line the likely reason a container stopped. */
const TROUBLE = /\b(error|err|fail(ed|ure)?|fatal|panic|exception|refused|denied|cannot|can't|unable|not found|timeout|timed out|killed)\b/i;

/** The line that best says why a container stopped: the last troubled one, else the last one. */
export function pickLine(lines: string[]): string | null {
	const kept = lines.map((l) => l.trim()).filter(Boolean);
	const line = [...kept].reverse().find((l) => TROUBLE.test(l)) ?? kept[kept.length - 1];
	if (!line) return null;
	return line.length > 160 ? `${line.slice(0, 157)}…` : line;
}

const cache = new Map<string, Promise<string | null>>();

/**
 * The last words of a container that keeps restarting, from the log of its previous run.
 * Asked once per pod and container; null when there is no such log or it cannot be read.
 */
export function lastLogLine(pod: ObjectRef, container: string | null): Promise<string | null> {
	const key = `${pod.context ?? ''}/${pod.namespace ?? ''}/${pod.name}/${container ?? ''}`;
	let found = cache.get(key);
	if (!found) {
		found = new Promise<string | null>((resolve) => {
			const lines: string[] = [];
			let stop = () => {};
			let finished = false;
			const done = () => {
				if (finished) return;
				finished = true;
				clearTimeout(timer);
				stop();
				resolve(pickLine(lines));
			};
			const timer = setTimeout(done, 5000);
			try {
				stop = backend.streamLogs(pod, { container, previous: true, follow: false, tailLines: 20 }, (event) => {
					if (event.type === 'lines') lines.push(...event.lines.map((l) => l.text));
					else if (event.type === 'end') done();
					else if (event.type === 'error' && !lines.length) done();
				});
				if (finished) stop();
			} catch {
				done();
			}
		});
		cache.set(key, found);
		if (cache.size > 50) cache.delete(cache.keys().next().value!);
	}
	return found;
}
