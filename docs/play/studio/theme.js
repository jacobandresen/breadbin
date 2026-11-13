// theme - the code editor's colours: retrobox, the gruvbox-flavoured scheme
// that ships with Vim and Neovim (colors/retrobox.vim, by Maxim Kim, after
// gruvbox8 and gruvbox by morhetz), dark variant. Each colour below is the
// one retrobox gives the Vim highlight group named beside it, so C reads
// exactly as it does in Neovim.

import { EditorView, HighlightStyle, syntaxHighlighting, tags as t } from './vendor/codemirror.js';

export const RETROBOX = {
  bg: '#1c1c1c', // Normal
  bg1: '#3c3836', // Pmenu
  bg2: '#504945', // PmenuSel, MatchParen, NonText
  line: '#303030', // CursorLine
  fg: '#ebdbb2', // Normal
  fg0: '#fbf1c7', // Cursor
  gray: '#a89984',
  comment: '#928374', // Comment
  lineNr: '#7c6f64', // LineNr
  red: '#fb5944', // Statement, Keyword, Conditional, Repeat, Label
  green: '#b8bb26', // String, Function (bold)
  yellow: '#fabd2f', // Type, Typedef, CursorLineNr
  blue: '#83a598', // Identifier
  purple: '#d3869b', // Constant, Number, Boolean, Character
  aqua: '#8ec07c', // PreProc, Include, Define, Macro, Operator, Structure
  orange: '#fe8019', // Special, Delimiter, StorageClass
  visual: '#2a405a', // Visual
  search: '#3a4a3a', // Search
};
const c = RETROBOX;

export const retroboxTheme = EditorView.theme({
  '&': { color: c.fg, backgroundColor: c.bg, height: '100%' },
  '.cm-scroller': { fontFamily: '"JetBrains Mono", ui-monospace, "SF Mono", Menlo, Consolas, monospace', fontSize: '13.5px', lineHeight: '1.6' },
  '.cm-content': { caretColor: c.fg0, padding: '10px 0' },
  '.cm-cursor, .cm-dropCursor': { borderLeftColor: c.fg0, borderLeftWidth: '2px' },
  '&.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection': { backgroundColor: c.visual },
  '.cm-activeLine': { backgroundColor: c.line },
  '.cm-gutters': { backgroundColor: c.bg, color: c.lineNr, border: 'none', paddingRight: '4px' },
  '.cm-activeLineGutter': { backgroundColor: c.bg, color: c.yellow, fontWeight: '700' },
  '.cm-lineNumbers .cm-gutterElement': { padding: '0 8px 0 14px', minWidth: '44px' },
  '.cm-foldGutter .cm-gutterElement': { color: c.bg2, transition: 'color 120ms' },
  '.cm-gutters:hover .cm-foldGutter .cm-gutterElement': { color: c.comment },
  '.cm-matchingBracket, &.cm-focused .cm-matchingBracket': { backgroundColor: c.bg2, color: 'inherit', fontWeight: '700', textDecoration: 'underline' },
  '.cm-nonmatchingBracket': { color: c.red },
  '.cm-searchMatch': { backgroundColor: c.search, outline: `1px solid ${c.green}55` },
  '.cm-searchMatch.cm-searchMatch-selected': { backgroundColor: '#4b5e3b' },
  '.cm-selectionMatch': { backgroundColor: '#ffffff12' },
  '.cm-tooltip': { backgroundColor: c.bg1, color: c.fg, border: `1px solid ${c.bg2}`, borderRadius: '8px', boxShadow: '0 12px 32px rgb(0 0 0 / 0.45)', overflow: 'hidden' },
  '.cm-tooltip-autocomplete > ul': { fontFamily: '"JetBrains Mono", ui-monospace, monospace', fontSize: '12.5px', maxHeight: '16em' },
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
  '.cm-gutter-lint': { width: '14px' },
  '.cm-placeholder': { color: c.comment },
  '.cm-specialChar': { color: c.red },
}, { dark: true });

// Vim's highlight groups, by the tags CodeMirror's C and assembly parsers give.
export const retroboxHighlight = HighlightStyle.define([
  { tag: [t.keyword, t.controlKeyword, t.operatorKeyword, t.definitionKeyword, t.moduleKeyword, t.self], color: c.red }, // Statement, Keyword
  { tag: [t.typeName, t.standard(t.typeName), t.className, t.namespace], color: c.yellow }, // Type
  { tag: [t.modifier], color: c.orange }, // StorageClass: static, const, volatile, extern
  { tag: [t.function(t.variableName), t.function(t.definition(t.variableName)), t.function(t.propertyName)], color: c.green, fontWeight: '700' }, // Function
  { tag: [t.string, t.special(t.string), t.docString], color: c.green }, // String
  { tag: [t.character, t.number, t.integer, t.float, t.bool, t.null, t.atom, t.constant(t.variableName), t.standard(t.variableName)], color: c.purple }, // Constant, Number
  { tag: [t.processingInstruction, t.macroName, t.meta, t.special(t.variableName)], color: c.aqua }, // PreProc, Macro
  { tag: [t.operator, t.derefOperator, t.arithmeticOperator, t.logicOperator, t.bitwiseOperator, t.compareOperator, t.updateOperator], color: c.aqua }, // Operator
  { tag: [t.bracket, t.paren, t.brace, t.squareBracket, t.angleBracket, t.separator, t.punctuation], color: c.orange }, // Delimiter
  { tag: [t.propertyName, t.attributeName], color: c.blue }, // Identifier (struct fields)
  { tag: [t.labelName], color: c.yellow, fontWeight: '700' }, // labels in assembly
  { tag: [t.variableName, t.name], color: c.fg },
  { tag: [t.comment, t.lineComment, t.blockComment], color: c.comment }, // Comment
  { tag: t.escape, color: c.red }, // SpecialChar
  { tag: t.invalid, color: c.red, fontWeight: '700' },
  { tag: t.heading, color: c.green, fontWeight: '700' }, // Title
  { tag: t.link, color: c.blue, textDecoration: 'underline' }, // Underlined
  { tag: t.strong, fontWeight: '700' },
  { tag: t.emphasis, fontStyle: 'italic' },
]);

export const retrobox = [retroboxTheme, syntaxHighlighting(retroboxHighlight)];
