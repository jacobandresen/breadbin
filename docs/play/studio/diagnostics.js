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

const plain = (s) => s.replace(/[‘’]/g, "'").replace(/\x1b\[[0-9;]*m/g, '');
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
