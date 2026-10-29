/** One line of a diff: kept, removed from the old text or added in the new one. */
export interface DiffLine {
	kind: 'same' | 'removed' | 'added';
	text: string;
	/** Line numbers in the old and the new text, from 1. */
	old: number | null;
	new: number | null;
}

/** Lines of a diff with unchanged stretches folded, as `kubectl diff` shows them. */
export type DiffPart = { kind: 'lines'; lines: DiffLine[] } | { kind: 'fold'; count: number };

/** The shortest line diff between two texts (Myers' algorithm). */
export function diffLines(before: string, after: string): DiffLine[] {
	const a = before === '' ? [] : before.replace(/\n$/, '').split('\n');
	const b = after === '' ? [] : after.replace(/\n$/, '').split('\n');
	const n = a.length;
	const m = b.length;
	const max = n + m;
	const offset = max + 1;
	const v = new Int32Array(2 * max + 3);
	const trace: Int32Array[] = [];
	let done = false;
	for (let d = 0; d <= max && !done; d++) {
		trace.push(v.slice());
		for (let k = -d; k <= d; k += 2) {
			let x = k === -d || (k !== d && v[offset + k - 1] < v[offset + k + 1]) ? v[offset + k + 1] : v[offset + k - 1] + 1;
			let y = x - k;
			while (x < n && y < m && a[x] === b[y]) {
				x++;
				y++;
			}
			v[offset + k] = x;
			if (x >= n && y >= m) {
				done = true;
				break;
			}
		}
	}
	// Walk back through the trace to find the edits.
	const out: DiffLine[] = [];
	let x = n;
	let y = m;
	for (let d = trace.length - 1; d >= 0; d--) {
		const vd = trace[d];
		const k = x - y;
		const prevK = k === -d || (k !== d && vd[offset + k - 1] < vd[offset + k + 1]) ? k + 1 : k - 1;
		const prevX = vd[offset + prevK];
		const prevY = prevX - prevK;
		while (x > prevX && y > prevY) {
			out.push({ kind: 'same', text: a[x - 1], old: x, new: y });
			x--;
			y--;
		}
		if (d > 0) {
			if (x === prevX) out.push({ kind: 'added', text: b[y - 1], old: null, new: y });
			else out.push({ kind: 'removed', text: a[x - 1], old: x, new: null });
		}
		x = prevX;
		y = prevY;
	}
	return out.reverse();
}

/** Whether a diff changes anything. */
export function changed(lines: DiffLine[]): boolean {
	return lines.some((l) => l.kind !== 'same');
}

/** Keeps `context` unchanged lines around each change and folds the rest. */
export function fold(lines: DiffLine[], context = 3): DiffPart[] {
	const keep = new Array<boolean>(lines.length).fill(false);
	lines.forEach((line, i) => {
		if (line.kind === 'same') return;
		for (let j = Math.max(0, i - context); j <= Math.min(lines.length - 1, i + context); j++) keep[j] = true;
	});
	const parts: DiffPart[] = [];
	let i = 0;
	while (i < lines.length) {
		const start = i;
		const kept = keep[i];
		while (i < lines.length && keep[i] === kept) i++;
		parts.push(kept ? { kind: 'lines', lines: lines.slice(start, i) } : { kind: 'fold', count: i - start });
	}
	return parts;
}
