import { describe, expect, it } from 'vitest';
import { changed, diffLines, fold } from './diff';

const kinds = (before: string, after: string) => diffLines(before, after).map((l) => `${l.kind[0]}${l.text}`);

describe('diffLines', () => {
	it('finds changed, added and removed lines', () => {
		expect(kinds('a\nb\nc', 'a\nB\nc\nd')).toEqual(['sa', 'rb', 'aB', 'sc', 'ad']);
		expect(kinds('', 'x')).toEqual(['ax']);
		expect(kinds('x\n', '')).toEqual(['rx']);
		expect(kinds('same', 'same')).toEqual(['ssame']);
	});

	it('numbers lines on both sides', () => {
		const lines = diffLines('a\nb', 'b\nc');
		expect(lines).toEqual([
			{ kind: 'removed', text: 'a', old: 1, new: null },
			{ kind: 'same', text: 'b', old: 2, new: 1 },
			{ kind: 'added', text: 'c', old: null, new: 2 }
		]);
		expect(changed(lines)).toBe(true);
		expect(changed(diffLines('a', 'a'))).toBe(false);
	});

	it('keeps diffs of large texts small', () => {
		const before = Array.from({ length: 2000 }, (_, i) => `line ${i}`).join('\n');
		const after = before.replace('line 1000', 'line 1000 changed');
		expect(diffLines(before, after).filter((l) => l.kind !== 'same')).toHaveLength(2);
	});
});

describe('fold', () => {
	it('folds unchanged stretches beyond the context', () => {
		const before = Array.from({ length: 20 }, (_, i) => `${i}`).join('\n');
		const after = before.replace('10', 'ten');
		const parts = fold(diffLines(before, after), 2);
		expect(parts.map((p) => (p.kind === 'fold' ? `fold ${p.count}` : `lines ${p.lines.length}`))).toEqual([
			'fold 8',
			'lines 6',
			'fold 7'
		]);
	});
});
