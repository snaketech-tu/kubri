import { describe, expect, it } from 'vitest';
import { inlines, parseMarkdown } from './markdown';

describe('parseMarkdown', () => {
	it('reads the blocks a model answers with', () => {
		const blocks = parseMarkdown(
			[
				'## Likely cause',
				'',
				'The container exits with **code 1** because `DATABASE_URL`',
				'is not set.',
				'',
				'1. Check the env:',
				'   more of step one',
				'2. Fix it',
				'   - nested',
				'',
				'```bash',
				'kubectl -n shop logs web-1 --previous',
				'```',
				'> Note this',
				'---'
			].join('\n')
		);
		expect(blocks.map((b) => b.type)).toEqual(['heading', 'paragraph', 'list', 'code', 'quote', 'rule']);
		expect(blocks[1]).toEqual({
			type: 'paragraph',
			inlines: [
				{ type: 'text', text: 'The container exits with ' },
				{ type: 'strong', text: 'code 1' },
				{ type: 'text', text: ' because ' },
				{ type: 'code', text: 'DATABASE_URL' },
				{ type: 'text', text: ' is not set.' }
			]
		});
		const list = blocks[2];
		expect(list.type === 'list' && list.items.map((i) => [i.inlines.map((x) => x.text).join(''), i.depth])).toEqual([
			['Check the env: more of step one', 0],
			['Fix it', 0],
			['nested', 1]
		]);
		expect(blocks[3]).toEqual({ type: 'code', lang: 'bash', text: 'kubectl -n shop logs web-1 --previous' });
	});

	it('keeps lists with blank lines between items together', () => {
		const blocks = parseMarkdown('- one\n\n- two\n\nAfter');
		expect(blocks.map((b) => b.type)).toEqual(['list', 'paragraph']);
	});

	it('links only to web pages and never makes HTML', () => {
		expect(inlines('[docs](https://kubernetes.io/docs) and [x](javascript:alert(1)) <b>hi</b>')).toEqual([
			{ type: 'link', text: 'docs', href: 'https://kubernetes.io/docs' },
			{ type: 'text', text: ' and [x](javascript:alert(1)) <b>hi</b>' }
		]);
		expect(inlines('snake_case_name and _this_')).toEqual([
			{ type: 'text', text: 'snake_case_name and ' },
			{ type: 'em', text: 'this' }
		]);
	});
});
