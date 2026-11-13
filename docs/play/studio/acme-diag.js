// acme-diag - ACME's messages as editor markers, like diagnostics.js for cc65:
//   Error - File code/main.asm, line 12 (Zone <untitled>): Value not defined (foo).
//   Warning - File main.asm, line 3 (Zone <untitled>): Label name starts with a shift-space.

const LINE = /^(Error|Warning|Serious error) - File (.+?), line (\d+)(?: \(.*?\))?: (.*)$/;

export function parseAcme(log) {
  const out = [];
  for (const raw of String(log).split('\n')) {
    const m = LINE.exec(raw.trim());
    if (m) out.push({ file: m[2], line: Number(m[3]), severity: /error/i.test(m[1]) ? 'error' : 'warning', message: m[4], tool: 'acme' });
  }
  return out;
}
