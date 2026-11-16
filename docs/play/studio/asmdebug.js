// asmdebug - what ACME says about an assembly program, made into what the
// Studio's debugger and live values read for C: line addresses in ld65's
// debug-file form (debuginfo.js), and the program's variables.
//
// ACME writes two listings when asked (-l, -r):
//
//   symbols (-l)    "\tspeed\t= $818\t; ?" - every global label and constant
//   report (-r)     "; ******** Source: code/main.asm" before each stretch
//                   of a file (an !source returns to the file it was in with
//                   another), then a line for each source line, in columns:
//                       "     9  0818 03                 speed   !byte 3"
//                   the line number (0-6), the address (8-12), the bytes it
//                   made (13-32; "..." after 8 of them), the source (32 on).
//
// A variable, in assembly, is a label on data (`speed !byte 3`, or the label
// alone with the data on the lines under it), or a constant naming a
// zero-page address that the code reads or writes (`counter = $fb`, then
// `inc counter`): the two ways intros and games keep their state.

const DATA = /^(?:!(?:by|byte|8|08|wo|word|16|le16|be16|fi|fill|skip|res)|\.(?:byte|word|res|addr))\b/i;
const WORDS = /^(?:!(?:wo|word|16|le16|be16)|\.(?:word|addr))\b/i;
const LABEL = /^([A-Za-z_]\w*):?(?=\s|$)/;

/** ACME's symbol list: { name: value }. */
export function parseSymbols(text) {
  const out = {};
  for (const m of String(text || '').matchAll(/^\s*([A-Za-z_]\w*)\s*=\s*\$([0-9a-f]+)/gim)) out[m[1]] = parseInt(m[2], 16);
  return out;
}

/** ACME's report: [{ file, line, addr (null: made nothing), size, text }], in the order it was assembled. */
export function parseReport(text) {
  const out = [];
  let file = '';
  for (const raw of String(text || '').split('\n')) {
    const src = /^; \*{8} Source: (.*)$/.exec(raw);
    if (src) {
      file = src[1].trim().replace(/^\.\//, '');
      continue;
    }
    const num = /^\s*(\d+)/.exec(raw.slice(0, 6));
    if (!num) continue;
    const addr = /^[0-9a-f]{4}$/i.test(raw.slice(8, 12)) ? parseInt(raw.slice(8, 12), 16) : null;
    const bytes = raw.slice(13, 32).trim();
    out.push({ file, line: Number(num[1]), addr, size: addr === null ? 0 : bytes.endsWith('...') ? null : bytes.length / 2, text: raw.slice(32) });
  }
  // the sizes ACME cut short ("..."): the values the line lists (or an !fill's
  // count), else up to where the next line's bytes go
  for (let i = 0; i < out.length; i++) {
    if (out[i].size !== null) continue;
    const t = body(out[i].text);
    const items = /^(?:!(?:by|byte|8|08)|\.byte)\s+(.*)$/i.exec(t)?.[1] ?? /^(?:!(?:wo|word|16)|\.word)\s+(.*)$/i.exec(t)?.[1];
    const fill = /^!(?:fi|fill)\s+(\d+|\$[0-9a-f]+)/i.exec(t)?.[1];
    if (items !== undefined && !/"/.test(items)) {
      out[i].size = items.split(',').length * (/^(?:!(?:wo|word|16)|\.word)/i.test(t) ? 2 : 1);
      continue;
    }
    if (fill) {
      out[i].size = fill[0] === '$' ? parseInt(fill.slice(1), 16) : Number(fill);
      continue;
    }
    const next = out.slice(i + 1).find((e) => e.addr !== null);
    const gap = next ? next.addr - out[i].addr : 0;
    out[i].size = gap > 8 && gap <= 0x1000 ? gap : 8;
  }
  return out;
}

const OPS = new Set('adc and asl bcc bcs beq bit bmi bne bpl brk bvc bvs clc cld cli clv cmp cpx cpy dec dex dey eor inc inx iny jmp jsr lda ldx ldy lsr nop ora pha php pla plp rol ror rti rts sbc sec sed sei sta stx sty tax tay tsx txa txs tya'.split(' '));
const isOp = (word) => OPS.has(word.toLowerCase());

/** A source line without its label and comment: what it does ('' for nothing). */
const body = (text) => {
  const t = text.replace(/;.*$/, ''); // (a ; in a string is rare in the code the Studio shows)
  const m = LABEL.exec(t);
  // a word at the line's start is a label, unless it is an instruction
  return (m && !isOp(m[1]) ? t.slice(m[0].length) : t).trim();
};
const labelOf = (text) => {
  const m = LABEL.exec(text.replace(/;.*$/, ''));
  return m && !isOp(m[1]) ? m[1] : null;
};
const isCode = (e) => e.addr !== null && e.size > 0 && isOp(body(e.text).slice(0, 3)) && !/^\w{4}/.test(body(e.text));
const isData = (e) => e.addr !== null && DATA.test(body(e.text));

/**
 * The debug-file text (ld65's form, for DebugInfo) of an ACME report: a line
 * record for each line of code, and a "function" for each label on code,
 * reaching to the next one, for the steps over and out.
 */
export function acmeDebug(entries, symbols = {}) {
  const files = [...new Set(entries.map((e) => e.file).filter(Boolean))];
  const out = ['seg\tid=0,name="CODE",start=0x000000,size=0x10000'];
  files.forEach((f, i) => out.push(`file\tid=${i},name="${f}"`));
  let span = 0;
  const code = entries.filter(isCode);
  for (const e of code) {
    out.push(`span\tid=${span},seg=0,start=${e.addr},size=${e.size}`);
    out.push(`line\tid=${span},file=${files.indexOf(e.file)},line=${e.line},type=0,span=${span}`);
    span++;
  }
  // a label on code (on its line, or alone on the line before the code) starts a routine
  const starts = [];
  for (let i = 0; i < entries.length; i++) {
    const name = labelOf(entries[i].text);
    if (!name || !(name in symbols)) continue;
    const at = entries.slice(i, i + 4).find((e) => e.addr !== null || body(e.text));
    if (at && isCode(at) && at.addr === symbols[name]) starts.push([name, at.addr]);
  }
  starts.sort((a, b) => a[1] - b[1]);
  const ends = [...new Set(entries.filter((e) => e.addr !== null && !isCode(e)).map((e) => e.addr))].sort((a, b) => a - b);
  starts.forEach(([name, a], i) => {
    // to the next routine, or to the first data after it, whichever is first
    const next = Math.min(starts[i + 1]?.[1] ?? 0x10000, ends.find((x) => x > a) ?? 0x10000);
    out.push(`span\tid=${span},seg=0,start=${a},size=${next - a}`);
    out.push(`scope\tid=${i},name="${name}",type=scope,size=${next - a},sym=${i},span=${span}`);
    span++;
  });
  return out.join('\n');
}

/**
 * The program's variables, as variables.js's globals() gives C's: [{ name,
 * addr, size, unit, signed }]. Labels on data (the data's bytes, to the next
 * label or the end of the data) and zero-page constants the code uses as
 * addresses (one byte each).
 */
export function asmVars(entries, symbols = {}) {
  const out = [];
  const seen = new Set();
  for (let i = 0; i < entries.length; i++) {
    const name = labelOf(entries[i].text);
    if (!name || seen.has(name) || !(name in symbols)) continue;
    // its data: on its line, or on the lines under a label alone (comments and blank lines between)
    const blank = (e) => !e.text.replace(/;.*$/, '').trim();
    let j = i;
    if (!body(entries[i].text)) {
      j++;
      while (j < entries.length && blank(entries[j])) j++;
    }
    const first = entries[j];
    if (!first || !isData(first) || first.addr !== symbols[name]) continue;
    let size = 0;
    for (let k = j; k < entries.length; k++) {
      const e = entries[k];
      if (k > j && (labelOf(e.text) || (e.addr !== null && !isData(e)))) break;
      if (isData(e)) size += e.size;
    }
    seen.add(name);
    out.push({ name, addr: symbols[name], size: Math.max(1, size), unit: WORDS.test(body(first.text)) ? 2 : 1, signed: false, file: first.file, line: first.line });
  }
  // `counter = $fb`, then used as an address: a zero-page variable
  const used = new Set();
  for (const e of entries) {
    const m = /^[a-z]{3}\s+\(?([A-Za-z_]\w*)\b/i.exec(body(e.text));
    if (m && isCode(e)) used.add(m[1]);
  }
  for (const [name, v] of Object.entries(symbols)) {
    if (seen.has(name) || v < 2 || v > 0xff || !used.has(name)) continue;
    out.push({ name, addr: v, size: 1, unit: 1, signed: false });
  }
  return out.sort((a, b) => a.addr - b.addr);
}
