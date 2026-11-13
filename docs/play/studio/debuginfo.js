// debuginfo - what ld65's debug file (--dbgfile) says about a program: which
// addresses each source line became, and which function each address is in.
// The debugger uses it both ways: a breakpoint on line 12 of src/main.c is a
// breakpoint at that line's first address; the CPU stopped at $092C is in
// bb_frame(), on line 97 of gen/bb.c.
//
// The file is lines of records, "type<TAB>key=value,key=value", e.g.
//   file  id=2,name="src/main.c",size=94,...
//   line  id=8,file=2,line=3,type=1,span=3       type 1: a C line; 0: assembly
//   span  id=3,seg=0,start=17,size=10            bytes 17..26 of segment 0
//   seg   id=0,name="CODE",start=0x000840,...
//   scope id=1,name="_f",type=scope,size=36,sym=6,span=18+19
//   sym   id=6,name="_f",val=0x840,...
//
//   const dbg = new DebugInfo(text);
//   dbg.addrs('src/main.c', 12)   -> [0x92c]     where line 12 starts
//   dbg.lineAt(0x930)             -> { file: 'src/main.c', line: 12 }
//   dbg.funcAt(0x930)             -> { name: 'main', ranges: [[start, end], ...] }

/** One record's fields: { id: 2, name: 'src/main.c', span: '3+4', ... } (numbers where they are numbers). */
function fields(rest) {
  const out = {};
  for (const m of rest.matchAll(/(\w+)=("[^"]*"|[^,]*)/g)) {
    const v = m[2];
    out[m[1]] = v.startsWith('"') ? v.slice(1, -1) : /^(0x[0-9a-f]+|\d+)$/i.test(v) ? Number(v) : v;
  }
  return out;
}

/** The ids in a "3+4+9" list. */
const ids = (v) => (v === undefined || v === '' ? [] : String(v).split('+').map(Number));

export class DebugInfo {
  /**
   * Read ld65's debug file. Lines of the files cc65 wrote (.build/*.s) are
   * left out: their C lines stand for them. `work` is a folder prefix to take
   * off the files' names (the build's working folder).
   */
  constructor(text = '', { work = '' } = {}) {
    const recs = { file: new Map(), line: [], span: new Map(), seg: new Map(), scope: [], sym: new Map() };
    for (const raw of String(text).split('\n')) {
      const tab = raw.indexOf('\t');
      if (tab < 0) continue;
      const type = raw.slice(0, tab);
      const f = fields(raw.slice(tab + 1));
      if (type === 'line' || type === 'scope') recs[type].push(f);
      else if (recs[type] instanceof Map) recs[type].set(f.id, f);
    }
    const name = (id) => {
      let n = recs.file.get(id)?.name || '';
      if (work && n.startsWith(`${work}/`)) n = n.slice(work.length + 1);
      return n.replace(/^\.\//, '');
    };
    const spanAt = (id) => {
      const s = recs.span.get(id);
      const seg = s && recs.seg.get(s.seg);
      return seg ? [seg.start + s.start, seg.start + s.start + s.size] : null;
    };

    /** file -> line -> [first addresses] (a line can have several: a for loop's start and its step) */
    this.files = new Map();
    /** [start, end, file, line], for lineAt */
    this.ranges = [];
    for (const l of recs.line) {
      const file = name(l.file);
      const c = l.type === 1;
      // a C line, or a line of a project's own assembly file (not cc65's output, not a macro)
      if (!(c || (l.type === undefined || l.type === 0)) || /^\.build\//.test(file) || !file || file.startsWith('/')) continue;
      if (!c && !/\.(s|asm|a65)$/i.test(file)) continue;
      for (const sid of ids(l.span)) {
        const r = spanAt(sid);
        if (!r || r[1] <= r[0]) continue;
        if (!this.files.has(file)) this.files.set(file, new Map());
        const lines = this.files.get(file);
        if (!lines.has(l.line)) lines.set(l.line, []);
        if (!lines.get(l.line).includes(r[0])) lines.get(l.line).push(r[0]);
        this.ranges.push([r[0], r[1], file, l.line]);
      }
    }
    for (const lines of this.files.values()) for (const a of lines.values()) a.sort((x, y) => x - y);
    // the narrowest range first, so lineAt finds the most precise line
    this.ranges.sort((a, b) => a[0] - b[0] || a[1] - a[0] - (b[1] - b[0]));
    /** address -> { file, line }, for the lines' first addresses */
    this.starts = new Map();
    for (const [file, lines] of this.files) for (const [line, addrs] of lines) for (const a of addrs) if (!this.starts.has(a)) this.starts.set(a, { file, line });

    /** functions: { name, ranges: [[start, end]...] } (C names, without cc65's "_") */
    this.funcs = [];
    for (const s of recs.scope) {
      if (s.type !== 'scope' || s.sym === undefined) continue;
      const ranges = ids(s.span).map(spanAt).filter(Boolean);
      if (ranges.length) this.funcs.push({ name: String(s.name).replace(/^_/, ''), ranges });
    }
  }

  /** The first addresses of `file`'s line `line` ([] if it made no code). */
  addrs(file, line) {
    return this.files.get(file)?.get(line) || [];
  }

  /** The lines of `file` that made code, in order. */
  codeLines(file) {
    return [...(this.files.get(file)?.keys() || [])].sort((a, b) => a - b);
  }

  /** The first line from `line` on (within a few) that made code: where a breakpoint on `line` goes. */
  snap(file, line, reach = 8) {
    for (let l = line; l <= line + reach; l++) if (this.addrs(file, l).length) return l;
    return null;
  }

  /** Is `addr` where a line starts? */
  isStart(addr) {
    return this.starts.has(addr);
  }

  /** The source line `addr` is part of: { file, line } or null. */
  lineAt(addr) {
    const s = this.starts.get(addr);
    if (s) return s;
    let best = null;
    for (const [a, b, file, line] of this.ranges) {
      if (a > addr) break;
      if (addr < b && (!best || b - a < best.size)) best = { file, line, size: b - a };
    }
    return best && { file: best.file, line: best.line };
  }

  /** The function `addr` is in: { name, ranges } or null. */
  funcAt(addr) {
    return this.funcs.find((f) => f.ranges.some(([a, b]) => addr >= a && addr < b)) || null;
  }

  /**
   * Every line's first address, or those inside function `fn` (from funcAt):
   * where a step can stop. The runtime's assembly (gen/*.s) is left out,
   * where each instruction is a line: a step goes through it in one.
   */
  lineStarts(fn = null) {
    const all = [...this.starts].filter(([, s]) => !/^gen\/.*\.s$/i.test(s.file)).map(([a]) => a);
    if (!fn) return all;
    return all.filter((a) => fn.ranges.some(([s, e]) => a >= s && a < e));
  }
}
