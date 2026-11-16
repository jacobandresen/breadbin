// diagnostics - turn what cc65, ca65 and ld65 print into editor markers:
// { file, line, severity: 'error' | 'warning' | 'info', message, tool }.
//
//   src/main.c:12: Error: Undeclared identifier 'b'       (cc65)
//   gen/bbsound.s:40: Warning: ...                        (ca65)
//   main.s:28: Warning: Unresolved external '_foo'        (ld65)
//   ld65: Error: 1 unresolved external(s) found - ...     (ld65)
//
// The tools quote with typographic quotes (‘x’); messages are normalised to
// plain ones. An unresolved external is reported against the assembly cc65
// generated, so it is moved back to the C file and line that uses the name.

const LINE = /^(.+?):(\d+): (Error|Warning|Fatal(?: error)?|Note): (.*)$/;
// "this" is how a tool names itself when run without an argv[0] of its own
const TOOL = /^(cc65|ca65|ld65|this): (Error|Warning|Fatal(?: error)?): (.*)$/;

const plain = (s) => s.replace(/[‘’]/g, "'").replace(/\x1b\[[0-9;]*m/g, ''); // eslint-disable-line no-control-regex -- (a terminal's colour codes)
const severity = (word) => (/error|fatal/i.test(word) ? 'error' : /warning/i.test(word) ? 'warning' : 'info');

/**
 * Parse `log` (one tool's output) into diagnostics. `sources` (path -> text)
 * lets an unresolved external from a cc65-generated `.s` point at the C line
 * that calls it; `generated` maps such a `.s` path back to its `.c`.
 */
export function parseDiagnostics(log, { tool = '', sources = {}, generated = {} } = {}) {
  const out = [];
  for (const raw of log.split('\n')) {
    const text = plain(raw).trim();
    if (!text || /errors? and \d+ warnings? generated/.test(text)) continue;
    let m = LINE.exec(text);
    if (m) {
      let [, file, line, word, message] = m;
      line = Number(line);
      const ext = /^Unresolved external '_?([^']+)'/.exec(message);
      if (ext) {
        const name = ext[1];
        const csrc = generated[file];
        message = `'${name}' is used but never defined (missing function body, or a typo?)`;
        const found = findUse(sources, name, csrc);
        if (found) [file, line] = found;
        else if (csrc) file = csrc;
        word = 'Error';
      }
      out.push({ file, line, severity: severity(word), message, tool });
      continue;
    }
    m = TOOL.exec(text);
    if (m) {
      // the unresolved externals were already reported one by one
      if (/unresolved external\(s\) found/.test(m[3]) && out.length) continue;
      out.push({ file: '', line: 0, severity: severity(m[2]), message: m[3], tool: m[1] === 'this' ? tool : m[1] });
    }
  }
  return out;
}

// "void jump(void);", "extern unsigned char __fastcall__ f(char x);" - but not "return f();"
const DECLARATION = /^\s*(?!return\b)(?:\w+[\s*]+)+\w+\s*\([^)]*\)\s*;/;

/** The first [file, line] where `name(` or `name` appears as a word, preferring `prefer`. */
function findUse(sources, name, prefer) {
  const re = new RegExp(`\\b${name.replace(/[^\w]/g, '')}\\b`);
  const files = [prefer, ...Object.keys(sources).filter((f) => f !== prefer && /\.(c|h)$/.test(f))].filter(Boolean);
  for (const f of files) {
    const lines = String(sources[f] ?? '').split('\n');
    // skip the declaration ("void foo(void);"), point at a use
    const use = lines.findIndex((l) => re.test(l) && !DECLARATION.test(l));
    if (use >= 0) return [f, use + 1];
  }
  return null;
}

/** "3 errors, 1 warning" */
export function summary(diags) {
  const n = (s) => diags.filter((d) => d.severity === s).length;
  const part = (k, word) => (k ? `${k} ${word}${k === 1 ? '' : 's'}` : '');
  return [part(n('error'), 'error'), part(n('warning'), 'warning')].filter(Boolean).join(', ') || 'no problems';
}

// cc65's and ld65's commonest messages, in plain words, with what usually
// fixes them: shown under the message in the problems list and in the editor.
const HINTS = [
  [/^';' expected/, 'A semicolon is missing: most likely at the end of the line before this one.'],
  [/^Undeclared identifier '(\w+)'/, (m) => `Nothing called ${m[1]} is declared. A typo? Or declare it first (for example "int ${m[1]};" above the function that uses it).`],
  [/^Call to undeclared function '(\w+)'/, (m) => `${m[1]}() isn't known here. A typo? Or define it above this line, or #include the header that declares it (bb_ calls need "bb.h").`],
  [/^'\)' expected/, 'A ( has no ) to close it on this line.'],
  [/^'\]' expected/, 'A [ has no ] to close it.'],
  [/^'}' expected/, 'A { has no } to close it: count the braces of the function this is in (a missing " on a line can cause this too).'],
  [/^Unexpected '}'/, 'One } too many, or a statement before it is not finished.'],
  [/^Too many arguments in function call/, 'The call gives more values than the function takes: compare it with the function\'s first line (F12 goes there).'],
  [/^Too few arguments in function call/, 'The call gives fewer values than the function takes: compare it with the function\'s first line (F12 goes there).'],
  [/^Statement has no effect/, 'This line works something out and throws it away. Did you mean = (store a value) instead of == (compare)?'],
  [/^Converting integer to pointer without a cast/, 'A number is stored in a pointer. Did you mean *p = ... (the place it points to), or &x (the address of x)?'],
  [/^Converting pointer to integer without a cast/, 'An address is stored in a number. Did you mean *p (what it points to)?'],
  [/^Conflicting types for '(\w+)'/, (m) => `${m[1]} is declared twice with different types: keep one (or give the other another name).`],
  [/^Assignment to rvalue/, 'The left side of = must be a variable (or *pointer, or array[i]): a number or a calculation can\'t be given a value.'],
  [/^Missing terminating " character/, 'A string has no closing ": add it before the end of the line.'],
  [/^Variable '(\w+)' is defined but never used/, (m) => `${m[1]} is never used: remove it, or use it.`],
  [/^Parameter '(\w+)' is never used/, (m) => `The function never uses ${m[1]}: harmless, but maybe something is missing.`],
  [/^Segment '(\w+)' overflows memory area '\w+' by (\d+) bytes/, (m) => `The program is ${m[2]} bytes too big for the C64's memory: make the biggest arrays smaller (the memory bar shows what takes the room).`],
  [/^Integer constant implies signed long/, 'This number is too big for an int (-32768 to 32767): add a U for unsigned (40000U), or check the number.'],
  [/^Expression expected/, 'Something is missing where a value should be: an extra comma or operator, or a missing value?'],
  [/^Result of comparison is always (true|false)/, (m) => `This test is always ${m[1]}: an unsigned char is 0 to 255, a signed char -128 to 127; is the type right?`],
];

/** A plain-words explanation of diagnostic `message`, or null. */
export function explain(message) {
  for (const [re, hint] of HINTS) {
    const m = re.exec(message || '');
    if (m) return typeof hint === 'function' ? hint(m) : hint;
  }
  return null;
}
