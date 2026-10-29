<script lang="ts">
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
	import { tags } from '@lezer/highlight';
	import { untrack } from 'svelte';

	let {
		value,
		readonly,
		onsave,
		oncancel,
		onchange
	}: {
		value: string;
		readonly: boolean;
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
		'&.cm-focused .cm-selectionBackground, .cm-selectionBackground, ::selection': {
			backgroundColor: 'color-mix(in srgb, var(--accent) 30%, transparent) !important'
		}
	});

	/** The current text, including unsaved edits. */
	export function text() {
		return view?.state.doc.toString() ?? value;
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
					}),
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
		return () => view?.destroy();
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
