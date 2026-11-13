// The parts of CodeMirror 6 the Studio uses, bundled into one ES module
// (docs/play/studio/vendor/codemirror.js) by bundle.mjs, so the page loads a
// single file and every package shares one copy of @codemirror/state.
export { EditorState, EditorSelection, StateEffect, StateField, Compartment, Prec, Annotation, Transaction } from '@codemirror/state';
export {
  EditorView, keymap, lineNumbers, highlightActiveLine, highlightActiveLineGutter, drawSelection,
  dropCursor, rectangularSelection, crosshairCursor, highlightSpecialChars, Decoration, ViewPlugin,
  WidgetType, gutter, GutterMarker, showPanel, hoverTooltip, placeholder,
} from '@codemirror/view';
export {
  HighlightStyle, syntaxHighlighting, StreamLanguage, LanguageSupport, indentOnInput, bracketMatching,
  foldGutter, foldKeymap, indentUnit, syntaxTree,
} from '@codemirror/language';
export { defaultKeymap, history, historyKeymap, indentWithTab, toggleComment } from '@codemirror/commands';
export {
  autocompletion, completionKeymap, closeBrackets, closeBracketsKeymap, snippetCompletion, completeFromList,
} from '@codemirror/autocomplete';
export { linter, lintGutter, setDiagnostics, lintKeymap, forceLinting } from '@codemirror/lint';
export { searchKeymap, highlightSelectionMatches, openSearchPanel } from '@codemirror/search';
export { cpp, cppLanguage } from '@codemirror/lang-cpp';
export { tags } from '@lezer/highlight';
