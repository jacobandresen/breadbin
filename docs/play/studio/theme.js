// theme - the code editor's look. In breadbin's own theme the colours are
// retrobox, the gruvbox-flavoured scheme that ships with Vim and Neovim
// (colors/retrobox.vim, by Maxim Kim, after gruvbox8 and gruvbox by morhetz),
// dark variant: each highlight below takes the colour retrobox gives the Vim
// group named beside it, so C reads exactly as it does in Neovim.

import { EditorView, HighlightStyle, syntaxHighlighting, tags as t } from './vendor/codemirror.js';

// The colours are the theme's --code-* tokens (themes/bakery.css has them
// for retrobox, beside each its Vim group), so a theme recolours the code
// without rebuilding the editors.
const NAMES = [
  'bg', 'bg1', 'bg2', 'line', 'fg', 'fg0', 'gray', 'comment', 'linenr', 'gutter',
  'red', 'green', 'function', 'yellow', 'blue', 'purple', 'aqua', 'operator', 'orange', 'visual', 'search',
];
const c = Object.fromEntries(NAMES.map((n) => [n, `var(--code-${n})`]));
/** Colour token `v` (a var()) at opacity `alpha`. */
const a = (v, alpha) => `rgb(from ${v} r g b / ${alpha})`;

/** A token's value, for what can't read var(): a canvas. */
export const token = (el, name) => getComputedStyle(el).getPropertyValue(name).trim();

const retroboxTheme = EditorView.theme({
  '&': { color: c.fg, backgroundColor: c.bg, height: '100%' },
  '.cm-scroller': { fontFamily: 'var(--code-font)', fontSize: 'max(var(--code-min, 0px), round(var(--studio-code-size, 13.5px), var(--code-step, 0.5px)))', lineHeight: '1.6' },
  '.cm-content': { caretColor: c.fg0, padding: '10px 0' },
  '.cm-cursor, .cm-dropCursor': { borderLeftColor: c.fg0, borderLeftWidth: '2px' },
  '&.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection': { backgroundColor: c.visual },
  '.cm-activeLine': { backgroundColor: c.line },
  '.cm-gutters': { backgroundColor: c.gutter, color: c.linenr, border: 'none', paddingRight: '4px' },
  '.cm-activeLineGutter': { backgroundColor: c.gutter, color: c.yellow, fontWeight: '700' },
  '.cm-lineNumbers .cm-gutterElement': { padding: '0 8px 0 14px', minWidth: '44px' },
  '.cm-foldGutter .cm-gutterElement': { color: c.bg2, transition: 'color 120ms' },
  '.cm-gutters:hover .cm-foldGutter .cm-gutterElement': { color: c.comment },
  '.cm-matchingBracket, &.cm-focused .cm-matchingBracket': { backgroundColor: c.bg2, color: 'inherit', fontWeight: '700', textDecoration: 'underline' },
  '.cm-nonmatchingBracket': { color: c.red },
  '.cm-searchMatch': { backgroundColor: c.search, outline: `1px solid ${a(c.green, 0.33)}` },
  '.cm-searchMatch.cm-searchMatch-selected': { backgroundColor: a(c.green, 0.35) },
  '.cm-selectionMatch': { backgroundColor: a(c.fg, 0.07) },
  '.cm-tooltip': { backgroundColor: c.bg1, color: c.fg, border: `1px solid ${c.bg2}`, borderRadius: '8px', boxShadow: '0 12px 32px rgb(0 0 0 / 0.45)', overflow: 'hidden' },
  '.cm-tooltip-autocomplete > ul': { fontFamily: 'var(--code-font)', fontSize: '12.5px', maxHeight: '16em' },
  '.cm-tooltip-autocomplete > ul > li': { padding: '3px 10px !important' },
  '.cm-tooltip-autocomplete > ul > li[aria-selected]': { backgroundColor: c.bg2, color: c.fg0 },
  '.cm-completionIcon': { opacity: '0.9', width: '1.4em' },
  '.cm-completionIcon-function::after': { content: '"ƒ"', color: c.green },
  '.cm-completionIcon-constant::after': { content: '"◆"', color: c.purple },
  '.cm-completionIcon-keyword::after': { content: '"⌘"', color: c.red },
  '.cm-completionIcon-type::after': { content: '"T"', color: c.yellow },
  '.cm-completionIcon-variable::after': { content: '"·"', color: c.blue },
  '.cm-completionIcon-text::after': { content: '"…"', color: c.gray },
  '.cm-completionDetail': { color: c.gray, fontStyle: 'normal', marginLeft: '1em' },
  '.cm-completionMatchedText': { color: c.yellow, textDecoration: 'none', fontWeight: '700' },
  '.cm-completionInfo': { padding: '8px 12px', maxWidth: '360px', fontFamily: 'var(--ui-font)', fontSize: '13px', lineHeight: '1.45' },
  '.cm-panels': { backgroundColor: c.bg1, color: c.fg, borderColor: c.bg2 },
  '.cm-panels.cm-panels-bottom': { borderTop: `1px solid ${c.bg2}` },
  '.cm-panel.cm-search': { padding: '6px 10px', fontFamily: 'var(--ui-font)' },
  '.cm-panel.cm-search input, .cm-panel.cm-search button': { backgroundColor: c.bg, color: c.fg, border: `1px solid ${c.bg2}`, borderRadius: '6px', padding: '3px 8px', fontSize: '12.5px' },
  '.cm-panel.cm-search label': { fontSize: '12.5px', color: c.gray },
  '.cm-textfield:focus': { outline: `1px solid ${c.yellow}` },
  '.cm-diagnostic': { padding: '6px 10px', fontFamily: 'var(--ui-font)', fontSize: '13px' },
  '.cm-diagnostic-error': { borderLeft: `3px solid ${c.red}` },
  '.cm-diagnostic-warning': { borderLeft: `3px solid ${c.orange}` },
  '.cm-lintRange-error': { backgroundImage: 'none', textDecoration: `underline wavy ${c.red}`, textUnderlineOffset: '3px' },
  '.cm-lintRange-warning': { backgroundImage: 'none', textDecoration: `underline wavy ${c.orange}`, textUnderlineOffset: '3px' },
  '.cm-lint-marker-error': { content: 'none' },
  // the debugger: breakpoints (red dots) in their own margin, the line the CPU stopped on (yellow)
  '.cm-bp-gutter': { cursor: 'pointer', minWidth: '16px' },
  '.cm-bp-gutter .cm-gutterElement': { display: 'flex', alignItems: 'center', justifyContent: 'center' },
  '.cm-bp': { display: 'block', width: '10px', height: '10px' },
  '.cm-bp-none': { visibility: 'hidden' },
  '.cm-bp-bp, .cm-bp-both': { borderRadius: '50%', backgroundColor: c.red, boxShadow: `0 0 6px ${a(c.red, 0.53)}` },
  '.cm-bp-both': { outline: `2px solid ${c.yellow}`, outlineOffset: '1px' },
  '.cm-bp-exec': { width: 0, height: 0, borderTop: '6px solid transparent', borderBottom: '6px solid transparent', borderLeft: `9px solid ${c.yellow}` },
  // live values: the running game's globals at the end of the lines that use them
  '.cm-live': { margin: '0 1.2em 0 2em', fontSize: '0.86em', color: c.comment, userSelect: 'none', whiteSpace: 'nowrap' },
  '.cm-live-item': { display: 'inline-flex', alignItems: 'baseline', gap: '0.35em', padding: '0 0.55em', marginRight: '0.4em', borderRadius: '999px', backgroundColor: a(c.purple, 0.08), border: `1px solid ${a(c.purple, 0.15)}` },
  '.cm-live-name': { color: c.gray },
  '.cm-live-name.graphable': { cursor: 'pointer' },
  '.cm-live-name.graphable:hover': { color: c.aqua, textDecoration: 'underline' },
  '.cm-live-value': { color: c.purple, fontWeight: '700', borderRadius: '3px' },
  '.cm-live-value.editable': { cursor: 'ew-resize', borderBottom: `1px dotted ${a(c.purple, 0.53)}` },
  '.cm-live-value.editable:hover, .cm-live-value.scrubbing': { color: c.fg0, backgroundColor: a(c.purple, 0.33) },
  '.cm-live-value.keep': { borderBottom: `1px solid ${a(c.yellow, 0.67)}` },
  // a value that just changed: yellow while it keeps changing, fading back once it settles
  '.cm-live-value.changed': { color: c.yellow, animation: 'cm-live-flash 400ms ease-out' },
  '@keyframes cm-live-flash': { from: { textShadow: `0 0 6px ${c.yellow}` }, to: { textShadow: 'none' } },
  '.cm-live-input': { font: 'inherit', fontWeight: '700', color: c.fg0, background: c.bg1, border: `1px solid ${c.purple}`, borderRadius: '3px', padding: '0 3px', outline: 'none', width: 'auto' },
  '.cm-exec-line': { backgroundColor: a(c.yellow, 0.16), boxShadow: `inset 3px 0 0 ${c.yellow}` },
  '&.cm-focused .cm-exec-line.cm-activeLine, .cm-exec-line.cm-activeLine': { backgroundColor: a(c.yellow, 0.22) },
  '.cm-gutter-lint': { width: '14px' },
  '.cm-placeholder': { color: c.comment },
  '.cm-specialChar': { color: c.red },
}, { dark: true });

// Vim's highlight groups, by the tags CodeMirror's C and assembly parsers give.
const retroboxHighlight = HighlightStyle.define([
  // (a look without colours to spare sets them apart by weight instead: --code-keyword-weight)
  { tag: [t.keyword, t.controlKeyword, t.operatorKeyword, t.definitionKeyword, t.moduleKeyword, t.self], color: c.red, fontWeight: 'var(--code-keyword-weight, 400)' }, // Statement, Keyword
  { tag: [t.typeName, t.standard(t.typeName), t.className, t.namespace], color: c.yellow, fontWeight: 'var(--code-keyword-weight, 400)' }, // Type
  { tag: [t.modifier], color: c.orange, fontWeight: 'var(--code-keyword-weight, 400)' }, // StorageClass: static, const, volatile, extern
  { tag: [t.function(t.variableName), t.function(t.definition(t.variableName)), t.function(t.propertyName)], color: c.function, fontWeight: '700' }, // Function
  { tag: [t.string, t.special(t.string), t.docString], color: c.green }, // String
  { tag: [t.character, t.number, t.integer, t.float, t.bool, t.null, t.atom, t.constant(t.variableName), t.standard(t.variableName)], color: c.purple }, // Constant, Number
  { tag: [t.processingInstruction, t.macroName, t.meta, t.special(t.variableName)], color: c.aqua }, // PreProc, Macro
  { tag: [t.operator, t.derefOperator, t.arithmeticOperator, t.logicOperator, t.bitwiseOperator, t.compareOperator, t.updateOperator], color: c.operator }, // Operator
  { tag: [t.bracket, t.paren, t.brace, t.squareBracket, t.angleBracket, t.separator, t.punctuation], color: c.orange }, // Delimiter
  { tag: [t.propertyName, t.attributeName], color: c.blue }, // Identifier (struct fields)
  { tag: [t.labelName], color: c.yellow, fontWeight: '700' }, // labels in assembly
  { tag: [t.variableName, t.name], color: c.fg },
  { tag: [t.comment, t.lineComment, t.blockComment], color: c.comment, fontStyle: 'var(--code-comment-style, normal)' }, // Comment
  { tag: t.escape, color: c.red }, // SpecialChar
  { tag: t.invalid, color: c.red, fontWeight: '700' },
  { tag: t.heading, color: c.green, fontWeight: '700' }, // Title
  { tag: t.link, color: c.blue, textDecoration: 'underline' }, // Underlined
  { tag: t.strong, fontWeight: '700' },
  { tag: t.emphasis, fontStyle: 'italic' },
]);

export const retrobox = [retroboxTheme, syntaxHighlighting(retroboxHighlight)];
