// asm-lang - syntax highlighting for 6502 assembly, in both dialects the
// Studio builds: ca65 (".byte", "@local:", "lda #<label") and ACME ("!byte",
// "* = $0801", "+macro"). A CodeMirror stream parser: labels, mnemonics,
// directives, numbers in $hex, %binary and decimal, strings and comments.

import { StreamLanguage } from './vendor/codemirror.js';

const MNEMONICS = new Set(('adc and asl bcc bcs beq bit bmi bne bpl brk bvc bvs clc cld cli clv cmp cpx cpy dec dex dey ' +
  'eor inc inx iny jmp jsr lda ldx ldy lsr nop ora pha php pla plp rol ror rti rts sbc sec sed sei sta stx sty tax tay ' +
  'tsx txa txs tya bra phx phy plx ply stz trb tsb lax sax dcp isc slo rla sre rra anc alr arr sbx las').split(' '));

export const asm6502 = StreamLanguage.define({
  name: 'asm6502',
  startState: () => ({ operand: false }),
  token(stream, state) {
    if (stream.sol()) state.operand = false;
    if (stream.eatSpace()) return null;
    const ch = stream.peek();
    if (ch === ';') {
      stream.skipToEnd();
      return 'comment';
    }
    if (ch === '"' || ch === "'") {
      const q = stream.next();
      let esc = false;
      let c;
      while ((c = stream.next()) != null) {
        if (c === q && !esc) break;
        esc = !esc && c === '\\';
      }
      return q === "'" ? 'character' : 'string';
    }
    if (stream.match(/^\$[0-9a-f]+/i) || stream.match(/^%[01]+/) || stream.match(/^0x[0-9a-f]+/i) || stream.match(/^\d+/)) return 'number';
    // directives: .byte (ca65), !byte (ACME)
    if (stream.match(/^[.!][a-z_][\w]*/i)) return 'meta';
    if (stream.match(/^#[<>]?/)) return 'operator';
    if (stream.match(/^[<>+\-*/&|^=~]+/)) return 'operator';
    if (stream.match(/^[(),[\]{}]/)) return 'punctuation';
    const word = stream.match(/^[@:]?[a-z_][\w.]*:?/i);
    if (word) {
      const w = word[0];
      if (w.endsWith(':')) return 'labelName';
      const lower = w.toLowerCase();
      if (!state.operand && MNEMONICS.has(lower)) {
        state.operand = true;
        return 'keyword';
      }
      if (/^[xya]$/i.test(w) && state.operand) return 'typeName'; // ,x ,y index registers
      // a name at the start of a line, with no colon: an ACME label
      if (!state.operand && stream.column() === 0) return 'labelName';
      state.operand = true;
      return 'variableName';
    }
    stream.next();
    return null;
  },
  languageData: { commentTokens: { line: ';' } },
});
