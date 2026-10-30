// hack - the Hack tab: write 6502 assembly, assemble it with ACME (the
// cross-assembler many C64 demos are made with, compiled to WebAssembly by
// `make acme`) and run it in the emulator beside the editor. The examples are
// real intros, fetched from their authors' GitHub repositories; edits are
// saved in the browser and can be downloaded.

import * as emu from './emu.js';
import * as store from './store.js';
import { prepare, patchedKernal } from './engine.js';
import { $, h } from './ui.js';

const raw = (repo, sha, dir = '') => `https://raw.githubusercontent.com/${repo}/${sha}/${dir}`;

const BLANK = `; Your own program. Edit, then Run (Ctrl+Enter).

* = $0801                       ; a BASIC line: 10 SYS 2064
    !byte $0b, $08, $0a, $00, $9e
    !text "2064"
    !byte 0, 0, 0

* = $0810
loop:
    inc $d020                   ; border colour
    inc $d021                   ; background colour
    jmp loop
`;

/** The projects: the user's own, then three intros in ACME syntax. */
const PROJECTS = [
  { id: 'blank', title: 'My program', entry: 'main.asm', files: { 'main.asm': BLANK } },
  {
    id: 'fairlight', title: 'Fairlight intro', entry: 'code/main.asm',
    root: raw('esshahn/c64-fairlight', '5531171c707803bb68a97389521910442f52241f'),
    credit: 'Fairlight\'s classic intro, disassembled and cleaned up by awsm', url: 'https://github.com/esshahn/c64-fairlight',
  },
  {
    id: 'eaglesoft', title: 'Eaglesoft intro', entry: 'code/main.asm',
    root: raw('esshahn/c64-eaglesoft', '6b5dedb05db899831d768b373702e06f94cecde1'),
    credit: 'The Eaglesoft Incorporated intro, disassembled by awsm', url: 'https://github.com/esshahn/c64-eaglesoft',
  },
  {
    id: 'haz', title: 'Can I Haz Member', entry: 'index.asm',
    root: raw('esshahn/c64-prods-by-awsm', 'aa95e1e9b4145839445cf39aa74259e1a7d0f1dd', 'can-i-haz-member/'),
    credit: 'A colour-wash intro by awsm of Mayday!, music by Laxity', url: 'https://github.com/esshahn/c64-prods-by-awsm',
  },
];

const INCLUDE = /^[ \t]*!(source|src|bin|binary)[ \t]+"([^"]+)"/gim;
const isText = (path) => /\.(asm|a|s|inc|txt)$/i.test(path);

/** Fetch a project's files: the entry and everything it includes. */
async function gather(p) {
  if (p.files) return { ...p.files };
  const files = {};
  const todo = [p.entry];
  while (todo.length) {
    const path = todo.shift();
    if (path in files) continue;
    const bytes = await store.cachedBytes(p.root + path);
    files[path] = isText(path) ? new TextDecoder('latin1').decode(bytes) : bytes;
    if (isText(path)) for (const [, , inc] of files[path].matchAll(INCLUDE)) todo.push(inc);
  }
  return files;
}

let acmeModule = null;

/** Assemble `files` from `entry` into a PRG. Returns { prg, log }. */
async function assemble(files, entry) {
  acmeModule ??= import('./acme.mjs').then((m) => m.default);
  const log = [];
  // a fresh instance per run: ACME keeps global state
  const acme = await (await acmeModule)({ print: (s) => log.push(s), printErr: (s) => log.push(s) });
  for (const [path, data] of Object.entries(files)) {
    acme.FS.mkdirTree(`/src/${path}`.replace(/\/[^/]*$/, ''));
    acme.FS.writeFile(`/src/${path}`, data);
  }
  acme.FS.chdir('/src');
  try {
    acme.callMain(['-f', 'cbm', '-o', '/out.prg', entry]);
  } catch {}
  let prg = null;
  try {
    prg = acme.FS.readFile('/out.prg');
  } catch {}
  return { prg, log: log.join('\n') };
}

// ── the tab ─────────────────────────────────────────────────────────────────

let project = null;
let files = {};
let original = {};
let path = '';
let session = null;

const saved = (id) => store.load(`hack.${id}`, {});
const dirty = () => Object.keys(saved(project.id)).length > 0;

export function initHack() {
  const root = $('#hack');
  const pick = h('select.hack-pick', { 'aria-label': 'Project' }, ...PROJECTS.map((p) => h('option', { value: p.id }, p.title)));
  const file = h('select.hack-file', { 'aria-label': 'File' });
  const editor = h('textarea.hack-editor', { spellcheck: false, autocapitalize: 'off', autocomplete: 'off', wrap: 'off' });
  const log = h('pre.hack-log');
  const stage = h('div.hack-stage', {}, h('p.hack-idle', {}, 'Press Run to assemble and start the program here.'));
  const credit = h('p.hack-credit');
  const state = h('span.hack-state');

  const keep = () => {
    if (!isText(path)) return;
    files[path] = editor.value;
    const edits = saved(project.id);
    if (editor.value === original[path]) delete edits[path];
    else edits[path] = editor.value;
    store.save(`hack.${project.id}`, edits);
    state.textContent = dirty() ? 'your edits are saved in this browser' : '';
  };
  const showFile = (p) => {
    path = p;
    file.value = p;
    editor.value = isText(p) ? files[p] : `(binary file, ${files[p].length} bytes)`;
    editor.readOnly = !isText(p);
  };
  const open = async (id) => {
    project = PROJECTS.find((p) => p.id === id) || PROJECTS[0];
    pick.value = project.id;
    store.save('hack.project', project.id);
    credit.replaceChildren(...(project.url ? [project.credit, ' · ', h('a', { href: project.url, target: '_blank', rel: 'noopener' }, 'source on GitHub ↗')] : ['Your own code, kept in this browser.']));
    editor.value = 'Loading…';
    try {
      original = await gather(project);
      files = { ...original, ...saved(project.id) };
    } catch (e) {
      editor.value = `Couldn't fetch the source: ${e.message}`;
      return;
    }
    file.replaceChildren(...Object.keys(files).map((p) => h('option', { value: p }, p)));
    state.textContent = dirty() ? 'your edits are saved in this browser' : '';
    showFile(project.entry);
  };
  const run = async () => {
    if (isText(path)) files[path] = editor.value;
    log.textContent = 'Assembling…';
    const { prg, log: out } = await assemble(files, project.entry);
    const ok = prg && `OK: ${prg.length} bytes from $${(prg[0] | (prg[1] << 8)).toString(16)}`;
    log.textContent = [out, ok].filter(Boolean).join('\n') || 'No output.';
    log.classList.toggle('bad', !prg);
    if (!prg) return;
    session?.stop();
    const run = await prepare('hack.prg', prg, { canMount: false });
    session = emu.run(stage, run, { kernal: await patchedKernal(store.cachedBytes) });
  };
  const download = () => {
    const blob = new Blob([isText(path) ? editor.value : files[path]]);
    const a = h('a', { href: URL.createObjectURL(blob), download: path.split('/').pop() });
    a.click();
    URL.revokeObjectURL(a.href);
  };
  const revert = async () => {
    if (!confirm(`Throw away your edits to ${project.title}?`)) return;
    store.save(`hack.${project.id}`, {});
    open(project.id);
  };

  pick.addEventListener('change', () => open(pick.value));
  file.addEventListener('change', () => (keep(), showFile(file.value)));
  editor.addEventListener('input', () => (state.textContent = 'unsaved'));
  editor.addEventListener('keydown', (e) => {
    if (e.key === 'Tab') {
      e.preventDefault();
      editor.setRangeText('    ', editor.selectionStart, editor.selectionEnd, 'end');
    } else if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
      e.preventDefault();
      keep();
      run();
    } else if ((e.ctrlKey || e.metaKey) && e.key === 's') {
      e.preventDefault();
      keep();
    }
  });
  // "Error - File code/main.asm, line 12 ..." jumps to that line
  log.addEventListener('click', () => {
    const line = log.textContent.split('\n').find((l) => /File .+, line \d+/.test(l));
    const m = line && /File (.+?), line (\d+)/.exec(line);
    if (!m || !(m[1] in files)) return;
    showFile(m[1]);
    const at = editor.value.split('\n').slice(0, m[2] - 1).join('\n').length + 1;
    editor.focus();
    editor.setSelectionRange(at, at);
  });

  root.append(
    h('div.hack-bar', {}, pick, file,
      h('button.key.pill.primary', { onclick: () => (keep(), run()), title: 'Ctrl+Enter' }, '▶ Run'),
      h('button.key.pill', { onclick: keep, title: 'Ctrl+S' }, 'Save'),
      h('button.key.pill', { onclick: download }, 'Download'),
      h('button.key.pill', { onclick: revert }, 'Revert'),
      state),
    h('div.hack-body', {}, h('div.hack-code', {}, editor, log), h('div.hack-run', {}, stage, credit,
      h('p.hack-credit', {}, 'Assembled by ', h('a', { href: 'https://github.com/meonwax/acme', target: '_blank', rel: 'noopener' }, 'ACME 0.96.4'), ', Marco Baye\'s cross-assembler (GPL-2.0), in your browser.'))),
  );
  open(store.load('hack.project', 'blank'));
  addEventListener('breadbin:launch', () => (session?.stop(), (session = null)));
}
