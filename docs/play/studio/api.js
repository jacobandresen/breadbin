// api - what the code editor knows about the runtime: every function and
// constant in bb.h, with the comment written above it, for completion and
// hover. Read from bb.h itself, so the help can't drift from the code.

/**
 * Entries of `header` (bb.h's text): { name, kind: 'function' | 'constant' | 'macro',
 * signature, doc }. A doc is the comment just above the declaration.
 */
export function parseApi(header) {
  const out = [];
  const lines = header.split('\n');
  let doc = '';
  let inEnum = false;
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i].trim();
    const c = /^\/\*\s?(.*?)\s*(\*\/)?$/.exec(line);
    if (c && !line.startsWith('/* ──')) {
      // collect a comment, possibly spread over lines
      let text = c[1];
      let j = i;
      while (!lines[j].includes('*/') && j + 1 < lines.length) text += ` ${lines[++j].trim().replace(/^\*\s?/, '').replace(/\*\/$/, '')}`;
      doc = text.replace(/\*\/$/, '').trim();
      i = j;
      continue;
    }
    if (!line) {
      doc = '';
      continue;
    }
    let m = /^#define\s+(bb_\w+)\(([^)]*)\)/.exec(line);
    if (m) {
      out.push({ name: m[1], kind: 'function', signature: `${m[1]}(${m[2]})`, doc });
      doc = '';
      continue;
    }
    m = /^#define\s+(BB_\w+)\s+(\S+)/.exec(line);
    if (m && m[1] !== 'BB_H') {
      out.push({ name: m[1], kind: 'constant', signature: `${m[1]} = ${m[2]}`, doc });
      continue;
    }
    m = /^(?:extern\s+)?([\w\s*]+?)\s*(?:__fastcall__\s+)?\b(bb_\w+)\s*\(([^)]*)\)\s*;/.exec(line.replace(/\s+/g, ' ')) ||
      (/\(\s*$|,\s*$/.test(line) && /^(?:extern\s+)?([\w\s*]+?)\s*(?:__fastcall__\s+)?\b(bb_\w+)\s*\(([^)]*)\)\s*;/.exec(joinDecl(lines, i)));
    if (m) {
      out.push({ name: m[2], kind: 'function', signature: `${m[1].replace(/__fastcall__/, '').trim()} ${m[2]}(${m[3].replace(/\s+/g, ' ').trim()})`, doc });
      while (!lines[i].includes(';')) i++;
      doc = '';
      continue;
    }
    m = /^extern\s+(.+?)\b(bb_\w+)(\[\d*\])?\s*(?:,\s*(bb_\w+))?\s*;/.exec(line);
    if (m) {
      for (const name of [m[2], m[4]].filter(Boolean)) out.push({ name, kind: 'variable', signature: `${m[1].trim()} ${name}${m[3] || ''}`, doc });
      doc = '';
      continue;
    }
    if (/^enum\b/.test(line)) inEnum = true;
    if (inEnum) {
      for (const name of line.match(/BB_[A-Z]+/g) || []) out.push({ name, kind: 'constant', signature: `${name} (a C64 colour)`, doc });
      if (line.includes('}')) inEnum = false;
    }
  }
  return out;
}

function joinDecl(lines, i) {
  let s = '';
  for (let j = i; j < lines.length; j++) {
    s += ` ${lines[j].trim()}`;
    if (lines[j].includes(';')) break;
  }
  return s.trim().replace(/\s+/g, ' ');
}

/**
 * The functions and constants a C header declares (cc65's <stdlib.h>,
 * <string.h>, <c64.h>...), for completion: { name, kind, signature, doc, from }.
 * A doc is the comment just above (or at the end of) the declaration.
 * Names starting with "_" (the library's own) are left out.
 */
export function parseHeader(text, from = '') {
  const out = [];
  const seen = new Set();
  const lines = String(text).split('\n');
  let doc = '';
  for (let i = 0; i < lines.length; i++) {
    let line = lines[i].trim();
    if (/^\/\*/.test(line)) {
      let t = line;
      while (!lines[i].includes('*/') && i + 1 < lines.length) t += ` ${lines[++i].trim()}`;
      doc = t.replace(/^\/\*+|\*+\/$/g, '').replace(/\s*\*\s+/g, ' ').trim();
      if (/^[=*\s-]*$/.test(doc) || /Copyright|warranty|This software is provided/i.test(doc)) doc = '';
      continue;
    }
    if (!line) {
      doc = '';
      continue;
    }
    const tail = /\/\*\s*(.*?)\s*\*\/\s*$/.exec(line)?.[1] || '';
    let m = /^#\s*define\s+([A-Za-z]\w*)(\([^)]*\))?\s+(.*?)\s*(\/\*.*)?$/.exec(line);
    if (m && !/_H$/.test(m[1])) {
      const name = m[1];
      if (!seen.has(name)) out.push({ name, kind: m[2] ? 'function' : 'constant', signature: m[2] ? `${name}${m[2]}` : `${name} = ${m[3]}`, doc: tail || doc, from });
      seen.add(name);
      continue;
    }
    if (/^(typedef|struct|union|enum|extern\s+"C"|#)/.test(line)) {
      doc = '';
      continue;
    }
    // a prototype, maybe over several lines
    if (/\(/.test(line) && !/;/.test(line)) {
      let j = i;
      while (!/;/.test(line) && j + 1 < lines.length && j - i < 6) line += ` ${lines[++j].trim()}`;
      i = j;
    }
    m = /^((?:extern\s+)?[\w\s*]+?)\s*\b(?:__fastcall__\s+)?([A-Za-z]\w*)\s*\(([^)]*)\)\s*;/.exec(line.replace(/\s+/g, ' '));
    if (m && !/^(return|if|while)$/.test(m[2])) {
      const name = m[2];
      const type = m[1].replace(/\bextern\b|__fastcall__/g, '').trim();
      if (!seen.has(name)) out.push({ name, kind: 'function', signature: `${type} ${name}(${m[3].replace(/__fastcall__/g, '').trim()})`, doc: tail || doc, from });
      seen.add(name);
    }
    doc = '';
  }
  return out;
}

/** The headers a C file includes with <...>: ['stdlib.h', 'c64.h'] */
export const systemIncludes = (text) => [...String(text).matchAll(/^\s*#\s*include\s*<([\w/.]+)>/gm)].map((m) => m[1]);

/** The constants gen/assets.h defines: { name, value, group } (group: "sprite images", ...). */
export function parseAssetNames(header) {
  const out = [];
  let group = '';
  for (const line of String(header || '').split('\n')) {
    const g = /^\/\* (.+?)(?:, for .*)? \*\/$/.exec(line.trim());
    if (g) group = g[1];
    const m = /^#define\s+((?:SPR|TILE|FLAG|ROOM|SONG|SFX)_\w+)\s+(\S+)/.exec(line);
    if (m) out.push({ name: m[1], value: m[2], group });
  }
  return out;
}

export const C_KEYWORDS = ('auto break case char const continue default do double else enum extern float for goto if inline int long ' +
  'register return short signed sizeof static struct switch typedef union unsigned void volatile while __fastcall__').split(' ');
