// toolchain - build a C64 program from C and ca65 assembly with cc65, ca65
// and ld65 compiled to WebAssembly (tools/studio/cc65.sh). The same code runs
// in the page's build worker and in Node for the tests.
//
//   const tc = await Toolchain.load();
//   const { ok, prg, diagnostics, memory } = await tc.build({ 'src/main.c': '...' });
//
// Every `.c` file is compiled to assembly, every `.s` file (the generated ones
// too) is assembled, and the objects are linked with the C64 runtime library
// into a PRG that starts with `RUN`. The program may use $0801-$BFFF: the Studio's
// runtime keeps the screen, the tiles and the sprites at $C000-$FFFF (VIC bank 3),
// so the linker is told RAM ends at $C000.

import { resource } from './resource.js';
import { gunzip, untar } from './tar.js';
import { parseDiagnostics } from './diagnostics.js';

const TOOLS = ['cc65', 'ca65', 'ld65'];
const SYS = '/sys';
const WORK = '/p';
const enc = new TextEncoder();

/** Is `path` something the build compiles? (`tests/` holds host-side tests.) */
export const isSource = (path) => /\.(c|s)$/i.test(path) && !/^tests?\//.test(path);

export class Toolchain {
  constructor(factories, modules, sysroot) {
    this.factories = factories;
    this.modules = modules;
    this.sysroot = sysroot;
  }

  /** Fetch and compile the three tools and unpack the sysroot. */
  static async load() {
    const factories = {};
    const modules = {};
    await Promise.all(TOOLS.map(async (t) => {
      const [mod, bytes] = await Promise.all([import(`./cc65/${t}.mjs`), resource(`cc65/${t}.wasm`)]);
      factories[t] = mod.default;
      modules[t] = await WebAssembly.compile(bytes);
    }));
    const sysroot = untar(await gunzip(await resource('cc65/sysroot.tar.gz')));
    return new Toolchain(factories, modules, sysroot);
  }

  /**
   * Run one tool with `args` in a fresh instance (the tools keep global state)
   * over `files` (path -> text or bytes, absolute). Returns { code, log, read }.
   */
  async run(tool, args, files) {
    const log = [];
    const module = this.modules[tool];
    const m = await this.factories[tool]({
      print: (s) => log.push(s),
      printErr: (s) => log.push(s),
      instantiateWasm(imports, done) {
        WebAssembly.instantiate(module, imports).then((inst) => done(inst, module));
        return {};
      },
    });
    for (const [path, data] of Object.entries(files)) {
      m.FS.mkdirTree(path.replace(/\/[^/]*$/, '') || '/');
      m.FS.writeFile(path, typeof data === 'string' ? enc.encode(data) : data);
    }
    m.FS.mkdirTree(WORK);
    // the tools write into .build/<the source's folder>/ but don't create it
    for (const path of Object.keys(files)) if (path.startsWith(`${WORK}/`)) m.FS.mkdirTree(`${WORK}/.build/${path.slice(WORK.length + 1)}`.replace(/\/[^/]*$/, ''));
    m.FS.chdir(WORK);
    let code = 0;
    try {
      code = m.callMain(args) ?? 0;
    } catch (e) {
      code = typeof e?.status === 'number' ? e.status : 1;
      if (typeof e?.status !== 'number') log.push(`${tool}: Fatal error: ${e?.message || e}`);
    }
    return {
      code,
      log: log.join('\n'),
      read(path) {
        try {
          return m.FS.readFile(path);
        } catch {
          return null;
        }
      },
    };
  }

  /** The sysroot files under `dir` (include, asminc, cfg, lib), as absolute paths. */
  sys(dir) {
    const out = {};
    for (const [p, b] of Object.entries(this.sysroot)) if (p.startsWith(`${dir}/`)) out[`${SYS}/${p}`] = b;
    return out;
  }

  /**
   * Build `files` (project path -> text or bytes) into a PRG.
   * Options: `optimize` (cc65 -Oirs, default on), `staticLocals` (cc65 -Cl:
   * local variables live at fixed addresses, much faster on a 6502, but a
   * function can't call itself; default on), `defines` ({ NAME: value }),
   * `himem` (top of the program's RAM, default $C000).
   * Returns { ok, prg, diagnostics, log, memory, labels, sizes, debug, ms }: `labels`
 * are the program's symbols ({ name: address }), `sizes` its C variables'
 * sizes (dataSizes()), `debug` ld65's debug file (lines to addresses: debuginfo.js).
   */
  async build(files, { optimize = true, staticLocals = true, defines = {}, himem = 0xc000 } = {}) {
    const t0 = Date.now();
    const project = {};
    for (const [p, d] of Object.entries(files)) project[`${WORK}/${p}`] = d;
    const sources = Object.fromEntries(Object.entries(files).filter(([p, d]) => typeof d === 'string' && /\.(c|h)$/.test(p)));
    const dirs = [...new Set(Object.keys(files).map((p) => p.replace(/\/?[^/]*$/, '') || '.'))];
    const incs = dirs.flatMap((d) => ['-I', d]);
    const defs = Object.entries(defines).map(([k, v]) => `-D${k}=${v}`);
    const logs = [];
    const diagnostics = [];
    const generated = {};
    const objects = {};
    const sizes = {}; // the C variables' sizes, for the Variables panel
    const hidden = []; // ...and those of gen/, which it doesn't show
    const fail = () => ({ ok: false, prg: null, diagnostics, log: logs.join('\n'), memory: null, ms: Date.now() - t0 });

    const asm = {};
    for (const path of Object.keys(files).filter(isSource).sort()) {
      if (/\.s$/i.test(path)) {
        asm[path] = files[path];
        continue;
      }
      const out = `.build/${path.replace(/\.c$/i, '.s')}`;
      const r = await this.run('cc65', ['-t', 'c64', ...(optimize ? ['-Oirs'] : []), ...(staticLocals ? ['-Cl'] : []), '-g', '--include-dir', `${SYS}/include`, ...incs, ...defs, path, '-o', out],
        { ...this.sys('include'), ...project });
      logs.push(r.log);
      diagnostics.push(...parseDiagnostics(r.log, { tool: 'cc65' }));
      if (r.code !== 0) continue;
      asm[out] = r.read(`${WORK}/${out}`);
      generated[out] = path;
      const found = dataSizes(new TextDecoder().decode(asm[out]));
      Object.assign(sizes, found);
      // the runtime's and the assets' variables (gen/), which the Variables panel leaves out
      if (/^gen\//.test(path)) hidden.push(...Object.keys(found));
    }
    if (diagnostics.some((d) => d.severity === 'error')) return fail();

    for (const [path, data] of Object.entries(asm)) {
      const obj = `.build/${path.replace(/^\.build\//, '').replace(/\.s$/i, '.o')}`;
      const r = await this.run('ca65', ['--color', 'off', '-t', 'c64', '-g', '-I', `${SYS}/asminc`, ...incs, path, '-o', obj],
        { ...this.sys('asminc'), ...project, [`${WORK}/${path}`]: data });
      logs.push(r.log);
      diagnostics.push(...parseDiagnostics(r.log, { tool: 'ca65', sources, generated }));
      if (r.code === 0) objects[`${WORK}/${obj}`] = r.read(`${WORK}/${obj}`);
    }
    if (diagnostics.some((d) => d.severity === 'error')) return fail();

    const r = await this.run('ld65', ['--color', 'off', '-C', `${SYS}/cfg/c64.cfg`, '-D', `__HIMEM__=$${himem.toString(16)}`,
      '-m', 'game.map', '-Ln', 'game.lbl', '--dbgfile', 'game.dbg', '-o', 'game.prg', ...Object.keys(objects), `${SYS}/lib/c64.lib`],
    { ...this.sys('cfg'), ...this.sys('lib'), ...objects });
    logs.push(r.log);
    diagnostics.push(...parseDiagnostics(r.log, { tool: 'ld65', sources, generated }));
    const prg = r.code === 0 ? r.read(`${WORK}/game.prg`) : null;
    if (!prg) {
      if (!diagnostics.some((d) => d.severity === 'error')) diagnostics.push({ file: '', line: 0, severity: 'error', message: 'the linker made no program', tool: 'ld65' });
      return fail();
    }
    const map = new TextDecoder().decode(r.read(`${WORK}/game.map`) || new Uint8Array());
    const labels = parseLabels(new TextDecoder().decode(r.read(`${WORK}/game.lbl`) || new Uint8Array()));
    // the debugger's map of lines and addresses (debuginfo.js reads it)
    const debug = new TextDecoder().decode(r.read(`${WORK}/game.dbg`) || new Uint8Array());
    return { ok: true, prg, diagnostics, log: logs.filter(Boolean).join('\n'), memory: memoryUse(map, prg, himem), labels, sizes, hidden, debug, ms: Date.now() - t0 };
  }
}

/**
 * The sizes of the C variables in cc65's assembly `asm`: { name: { size, unit } }
 * ("_lives:" then ".res 1" is 1 byte; "_table:" then three ".word" lines is 6,
 * in units of 2). For the Variables panel, which knows addresses from the
 * labels but not how big each variable is.
 */
export function dataSizes(asm) {
  const out = {};
  let cur = null;
  const UNIT = { byte: 1, res: 1, word: 2, addr: 2, dword: 4 };
  for (const raw of String(asm).split('\n')) {
    const line = raw.replace(/;.*$/, '').trim();
    const label = /^(_\w+|[A-Z]\d{4}):$/.exec(line);
    if (label) {
      cur = label[1].startsWith('_') ? (out[label[1].slice(1)] = { size: 0, unit: 1 }) : null;
      continue;
    }
    const d = /^\.(byte|res|word|addr|dword)\s+(.*)$/.exec(line);
    if (d && cur) {
      if (d[1] === 'res') cur.size += parseInt(d[2].split(',')[0], d[2].startsWith('$') ? 16 : 10) || 0;
      else cur.size += UNIT[d[1]] * d[2].split(',').length;
      if (d[1] !== 'res' && d[1] !== 'byte') cur.unit = UNIT[d[1]];
    } else if (line && !d) cur = null; // code, a segment: the variable has ended
  }
  for (const k of Object.keys(out)) if (!out[k].size) delete out[k];
  return out;
}

/** ld65's VICE label file ("al 00C03A ._score") as { name: address }; C names keep their "_". */
export function parseLabels(text) {
  const out = {};
  for (const m of text.matchAll(/^al ([0-9A-F]+) \.(\S+)$/gim)) out[m[2]] = parseInt(m[1], 16);
  return out;
}

/**
 * How much of the program's RAM a build uses, from ld65's map file:
 * { segments: [{ name, start, size }], used, free, total } (bytes).
 */
export function memoryUse(map, prg, himem = 0xc000) {
  const segments = [];
  const list = /Segment list:\s*\n-+\n(?:Name.*\n-+\n)?([\s\S]*?)\n\s*\n/.exec(map);
  for (const line of (list?.[1] || '').split('\n')) {
    const m = /^(\w+)\s+([0-9A-F]{6})\s+([0-9A-F]{6})\s+([0-9A-F]{6})/i.exec(line.trim());
    if (m) segments.push({ name: m[1], start: parseInt(m[2], 16), size: parseInt(m[4], 16) });
  }
  const start = 0x0801;
  const total = himem - start;
  const end = segments.reduce((e, s) => Math.max(e, s.start + s.size), prg ? start + prg.length - 2 : start);
  const used = Math.min(total, end - start);
  return { segments, used, free: total - used, total };
}
