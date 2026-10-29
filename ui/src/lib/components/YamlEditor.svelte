<script lang="ts">
	import { autocompletion, type CompletionContext, type CompletionResult } from '@codemirror/autocomplete';
	import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands';
	import { yaml } from '@codemirror/lang-yaml';
	import { HighlightStyle, foldGutter, foldKeymap, syntaxHighlighting } from '@codemirror/language';
	import { Compartment, EditorState } from '@codemirror/state';
	import {
		EditorView,
		highlightActiveLine,
		highlightActiveLineGutter,
		keymap,
		lineNumbers
	} from '@codemirror/view';
	import { linter, lintGutter } from '@codemirror/lint';
	import { tags } from '@lezer/highlight';
	import { untrack } from 'svelte';
	import { completionPath, fieldsAt, unknownFields, yamlPathAt, type FieldPath, type KindSchema } from '../schema';

	let {
		value,
		readonly,
		onsave,
		oncancel,
		onchange,
		schema = null,
		oncursor
	}: {
		value: string;
		readonly: boolean;
		/** The kind's schema, for completion and for flagging unknown fields. */
		schema?: KindSchema | null;
		/** The path of the field under the cursor, as it moves. */
		oncursor?: (path: FieldPath) => void;
		onsave?: (text: string) => void;
		/** Esc while editing; `changed` tells whether the text differs from where editing began. */
		oncancel?: (changed: boolean) => void;
		onchange?: (dirty: boolean) => void;
	} = $props();

	let host = $state<HTMLDivElement>();
	let view: EditorView | undefined;
	/** The text when editing began. */
	let original = '';
	const editable = new Compartment();
	const schemaHelp = new Compartment();

	const highlight = HighlightStyle.define([
		{ tag: tags.propertyName, color: 'var(--syntax-key)' },
		{ tag: [tags.string, tags.special(tags.string)], color: 'var(--syntax-string)' },
		{ tag: [tags.number, tags.bool, tags.null], color: 'var(--syntax-literal)' },
		{ tag: tags.comment, color: 'var(--text-muted)', fontStyle: 'italic' },
		{ tag: [tags.punctuation, tags.separator], color: 'var(--text-muted)' }
	]);

	const theme = EditorView.theme({
		'&': { height: '100%', fontSize: '12.5px', backgroundColor: 'var(--surface-1)', color: 'var(--text)' },
		'.cm-scroller': { fontFamily: 'var(--mono)', lineHeight: '1.5' },
		'.cm-gutters': { backgroundColor: 'var(--surface-1)', color: 'var(--text-muted)', border: 'none' },
		'.cm-activeLine, .cm-activeLineGutter': { backgroundColor: 'var(--surface-2)' },
		'&.cm-focused': { outline: 'none' },
		'.cm-cursor': { borderLeftColor: 'var(--accent)' },
		'.cm-tooltip': {
			border: '1px solid var(--border)',
			borderRadius: '8px',
			backgroundColor: 'var(--surface-1)',
			color: 'var(--text)',
			boxShadow: 'var(--shadow)',
			overflow: 'hidden'
		},
		'.cm-tooltip-autocomplete > ul': { fontFamily: 'var(--mono)', maxHeight: '16em' },
		'.cm-tooltip-autocomplete > ul > li[aria-selected]': { backgroundColor: 'var(--accent)', color: 'var(--on-accent)' },
		'.cm-completionDetail': { marginLeft: '1em', fontStyle: 'normal', opacity: '0.7' },
		'.cm-completionInfo': { maxWidth: '360px', padding: '6px 10px', fontSize: '12px', lineHeight: '1.45' },
		'.cm-diagnostic': { padding: '4px 8px', fontSize: '12px' },
		'.cm-lintRange-warning': { textDecorationColor: 'var(--warn)' },
		'&.cm-focused .cm-selectionBackground, .cm-selectionBackground, ::selection': {
			backgroundColor: 'color-mix(in srgb, var(--accent) 30%, transparent) !important'
		}
	});

	/** Keys the schema allows where the cursor is, as the user types one. */
	function complete(kind: KindSchema, context: CompletionContext): CompletionResult | null {
		const text = context.state.doc.toString();
		const at = completionPath(text, context.pos);
		if (!at || (at.from === context.pos && !context.explicit)) return null;
		const present = new Set(at.present);
		const options = fieldsAt(kind, at.path)
			.filter((field) => !present.has(field.name))
			.map((field) => ({
			label: field.name,
			detail: field.required ? `${field.type}, required` : field.type,
			info: field.description || undefined,
			type: 'property',
			apply: `${field.name}: `,
			boost: field.required ? 1 : 0
		}));
		return options.length ? { from: at.from, options, validFor: /^[A-Za-z0-9_.-]*$/ } : null;
	}

	function schemaExtensions(kind: KindSchema | null) {
		if (!kind) return [];
		return [
			autocompletion({ override: [(context) => complete(kind, context)], icons: false }),
			lintGutter(),
			linter(
				(v) =>
					unknownFields(v.state.doc.toString(), kind).map((problem) => ({ ...problem, severity: 'warning' as const })),
				{ delay: 400 }
			)
		];
	}

	let cursorTimer: ReturnType<typeof setTimeout> | undefined;
	/** Tells where the cursor is, once it rests: parsing on every keystroke would be wasted. */
	function followCursor(v: EditorView) {
		if (!oncursor) return;
		clearTimeout(cursorTimer);
		cursorTimer = setTimeout(() => oncursor?.(yamlPathAt(v.state.doc.toString(), v.state.selection.main.head)), 150);
	}

	/** The current text, including unsaved edits. */
	export function text() {
		return view?.state.doc.toString() ?? value;
	}

	/** Swaps in another draft, as after merging with changes on the server; undo brings the old one back. */
	export function replace(text: string) {
		view?.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: text } });
		view?.focus();
	}

	function save() {
		if (view && onsave) onsave(view.state.doc.toString());
		return true;
	}

	/** Reading, Esc hands the keys back to the list; editing, it cancels. */
	function escape(v: EditorView) {
		if (v.state.readOnly) v.contentDOM.blur();
		else oncancel?.(v.state.doc.toString() !== original);
		return true;
	}

	$effect(() => {
		if (!host) return;
		const [doc, ro] = untrack(() => [value, readonly] as const);
		view = new EditorView({
			parent: host,
			state: EditorState.create({
				doc,
				extensions: [
					lineNumbers(),
					// Tells whether the text differs from where editing began, for the unsaved-changes guard.
					EditorView.updateListener.of((update) => {
						if (update.docChanged && !update.state.readOnly) onchange?.(update.state.doc.toString() !== original);
						if (update.docChanged || update.selectionSet) followCursor(update.view);
					}),
					schemaHelp.of(schemaExtensions(untrack(() => schema))),
					foldGutter(),
					history(),
					highlightActiveLine(),
					highlightActiveLineGutter(),
					yaml(),
					syntaxHighlighting(highlight),
					theme,
					editable.of([EditorState.readOnly.of(ro), EditorView.editable.of(!ro)]),
					keymap.of([
						{ key: 'Mod-s', run: save, preventDefault: true },
						{ key: 'Escape', run: escape },
						indentWithTab,
						...defaultKeymap,
						...historyKeymap,
						...foldKeymap
					])
				]
			})
		});
		return () => {
			clearTimeout(cursorTimer);
			view?.destroy();
		};
	});

	$effect(() => {
		const kind = schema;
		view?.dispatch({ effects: schemaHelp.reconfigure(schemaExtensions(kind)) });
	});

	// Follow the object while reading; never clobber what the user is typing.
	$effect(() => {
		const text = value;
		if (view && readonly && view.state.doc.toString() !== text) {
			view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: text } });
		}
	});

	$effect(() => {
		const ro = readonly;
		if (!view) return;
		view.dispatch({
			effects: editable.reconfigure([EditorState.readOnly.of(ro), EditorView.editable.of(!ro)])
		});
		if (!ro) {
			original = view.state.doc.toString();
			onchange?.(false);
			view.focus();
		}
	});
</script>

<div class="editor" bind:this={host}></div>

<style>
	.editor {
		height: 100%;
		min-height: 0;
		overflow: hidden;
	}
</style>
