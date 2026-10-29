/**
 * A small Markdown reader for model answers: paragraphs, headings, lists, code blocks, quotes,
 * rules, and inline code, bold, italics and web links. It builds data for a component to show,
 * never HTML, so an answer cannot put markup or scripts into the page.
 */

export type Inline =
	| { type: 'text'; text: string }
	| { type: 'code'; text: string }
	| { type: 'strong'; text: string }
	| { type: 'em'; text: string }
	| { type: 'link'; text: string; href: string };

export interface ListItem {
	inlines: Inline[];
	/** 0 for the outer list, 1 for one level in. */
	depth: number;
}

export type Block =
	| { type: 'paragraph'; inlines: Inline[] }
	| { type: 'heading'; level: number; inlines: Inline[] }
	| { type: 'list'; ordered: boolean; start: number; items: ListItem[] }
	| { type: 'code'; lang: string; text: string }
	| { type: 'quote'; inlines: Inline[] }
	| { type: 'rule' };

const INLINE =
	/`([^`]+)`|\*\*([^*]+)\*\*|__([^_]+)__|\[([^\]]+)\]\((https?:\/\/[^\s)]+)\)|\*([^*\s][^*]*?)\*|(?<![\w])_([^_\s][^_]*?)_(?![\w])/g;

export function inlines(text: string): Inline[] {
	const out: Inline[] = [];
	let last = 0;
	for (const m of text.matchAll(INLINE)) {
		if (m.index > last) out.push({ type: 'text', text: text.slice(last, m.index) });
		if (m[1] !== undefined) out.push({ type: 'code', text: m[1] });
		else if (m[2] !== undefined || m[3] !== undefined) out.push({ type: 'strong', text: m[2] ?? m[3] });
		else if (m[4] !== undefined) out.push({ type: 'link', text: m[4], href: m[5] });
		else out.push({ type: 'em', text: m[6] ?? m[7] });
		last = m.index + m[0].length;
	}
	if (last < text.length) out.push({ type: 'text', text: text.slice(last) });
	return out;
}

const LIST_ITEM = /^(\s*)([-*+]|\d+[.)])\s+(.*)$/;

export function parseMarkdown(source: string): Block[] {
	const lines = source.replace(/\r\n?/g, '\n').split('\n');
	const blocks: Block[] = [];
	let paragraph: string[] = [];
	const flush = () => {
		if (paragraph.length) blocks.push({ type: 'paragraph', inlines: inlines(paragraph.join(' ')) });
		paragraph = [];
	};
	for (let i = 0; i < lines.length; i++) {
		const line = lines[i];
		const trimmed = line.trim();
		const fence = /^(```|~~~)\s*([\w+-]*)/.exec(trimmed);
		if (fence) {
			flush();
			const body: string[] = [];
			while (++i < lines.length && !lines[i].trim().startsWith(fence[1])) body.push(lines[i]);
			blocks.push({ type: 'code', lang: fence[2], text: body.join('\n') });
			continue;
		}
		if (!trimmed) {
			flush();
			continue;
		}
		const heading = /^(#{1,6})\s+(.*?)\s*#*$/.exec(trimmed);
		if (heading) {
			flush();
			blocks.push({ type: 'heading', level: heading[1].length, inlines: inlines(heading[2]) });
			continue;
		}
		if (/^([-*_])(\s*\1){2,}$/.test(trimmed)) {
			flush();
			blocks.push({ type: 'rule' });
			continue;
		}
		if (trimmed.startsWith('>')) {
			flush();
			const quote = [trimmed.replace(/^>\s?/, '')];
			while (i + 1 < lines.length && lines[i + 1].trim().startsWith('>'))
				quote.push(lines[++i].trim().replace(/^>\s?/, ''));
			blocks.push({ type: 'quote', inlines: inlines(quote.join(' ')) });
			continue;
		}
		const item = LIST_ITEM.exec(line);
		if (item) {
			flush();
			const ordered = /\d/.test(item[2]);
			const list: Extract<Block, { type: 'list' }> = {
				type: 'list',
				ordered,
				start: ordered ? parseInt(item[2]) : 1,
				items: []
			};
			const base = item[1].length;
			let text = item[3];
			let depth = 0;
			const push = () => list.items.push({ inlines: inlines(text), depth });
			while (i + 1 < lines.length) {
				const next = lines[i + 1];
				const nextItem = LIST_ITEM.exec(next);
				if (nextItem) {
					push();
					text = nextItem[3];
					depth = nextItem[1].length > base ? 1 : 0;
					i++;
				} else if (next.trim() && /^\s+/.test(next)) {
					// A wrapped line of the item above.
					text += ' ' + next.trim();
					i++;
				} else if (!next.trim() && LIST_ITEM.test(lines[i + 2] ?? '')) {
					i++;
				} else break;
			}
			push();
			blocks.push(list);
			continue;
		}
		paragraph.push(trimmed);
	}
	flush();
	return blocks;
}
