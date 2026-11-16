// storage - where a Studio project lives:
//
//   browser   IndexedDB in this browser: the default, nothing to set up
//   folder    a folder on your disk, through the File System Access API
//             (Chrome, Edge, Opera): every save writes the real files, so
//             git, your own editor and a native `make` see them; edits made
//             outside the browser are picked up while the Studio is open
//   zip       export and import, for browsers without folder access
//
// A folder's handle is remembered (in IndexedDB) so the next visit can
// reopen it; the browser asks once more for permission when it does.

import { isText, normalise, SKIP } from './project.js';

const DB = 'breadbin-studio';
const enc = new TextEncoder();

/** Can this browser open a folder on disk? */
export const canUseFolders = () => typeof globalThis.showDirectoryPicker === 'function';

// ── IndexedDB: the browser's own copies and the remembered folders ───────

let dbp = null;
function db() {
  dbp ??= new Promise((resolve, reject) => {
    // version 2 adds 'history': each project's saved versions (projectHistory)
    const req = indexedDB.open(DB, 2);
    req.onupgradeneeded = () => {
      const have = req.result.objectStoreNames;
      if (!have.contains('projects')) req.result.createObjectStore('projects');
      if (!have.contains('folders')) req.result.createObjectStore('folders');
      if (!have.contains('history')) req.result.createObjectStore('history');
    };
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  });
  return dbp;
}

async function tx(store, mode, fn) {
  const d = await db();
  return new Promise((resolve, reject) => {
    const t = d.transaction(store, mode);
    const req = fn(t.objectStore(store));
    t.oncomplete = () => resolve(req?.result);
    t.onerror = () => reject(t.error);
  });
}

/** A project kept in this browser, under `id`. */
export class BrowserStore {
  constructor(id) {
    this.id = id;
    this.kind = 'browser';
  }

  get label() {
    return 'this browser';
  }

  /** The project's files, or null if there is none yet. */
  async read() {
    const v = await tx('projects', 'readonly', (s) => s.get(this.id));
    return v ? v.files : null;
  }

  /** Save `changed` (path -> text/bytes; null deletes). */
  async write(changed) {
    const files = { ...((await this.read()) || {}) };
    for (const [p, d] of Object.entries(changed)) {
      if (d == null) delete files[p];
      else files[p] = d;
    }
    await tx('projects', 'readwrite', (s) => s.put({ files, at: Date.now() }, this.id));
  }

  async remove() {
    await tx('projects', 'readwrite', (s) => s.delete(this.id));
  }
}

// ── history: a project's versions, one kept at each Run that changed something ──

const MAX_VERSIONS = 40;

/** Project `key`'s saved versions, newest first: [{ at, label, files }]. */
export async function projectHistory(key) {
  return ((await tx('history', 'readonly', (s) => s.get(key))) || []).slice().reverse();
}

/**
 * Keep `files` as a version of project `key` (with `label`, say "Run"),
 * unless they are the same as the last one kept. The oldest go past 40.
 * Resolves to whether one was kept.
 */
export async function keepVersion(key, files, label) {
  const list = (await tx('history', 'readonly', (s) => s.get(key))) || [];
  const last = list[list.length - 1];
  const same = last && Object.keys(files).length === Object.keys(last.files).length && Object.entries(files).every(([p, d]) => sameData(last.files[p], d));
  if (same) return false;
  list.push({ at: Date.now(), label, files: { ...files } });
  await tx('history', 'readwrite', (s) => s.put(list.slice(-MAX_VERSIONS), key));
  return true;
}

const sameData = (a, b) => {
  if (typeof a === 'string' || typeof b === 'string') return a === b;
  if (!a || !b || a.length !== b.length) return false;
  for (let i = 0; i < a.length; i++) if (a[i] !== b[i]) return false;
  return true;
};

/** The ids of the projects kept in this browser. */
export const browserProjects = () => tx('projects', 'readonly', (s) => s.getAllKeys());

// ── a folder on disk ──────────────────────────────────────────────────────

/** A project in a folder on disk (a FileSystemDirectoryHandle). */
export class FolderStore {
  constructor(handle) {
    this.handle = handle;
    this.kind = 'folder';
    this.stamps = new Map(); // path -> lastModified, for changes()
  }

  get label() {
    return `${this.handle.name}/`;
  }

  /** Ask for the user's permission to read and write (needs a click the first time). */
  async allow(ask = true) {
    const opts = { mode: 'readwrite' };
    if ((await this.handle.queryPermission?.(opts)) === 'granted') return true;
    if (!ask) return false;
    return (await this.handle.requestPermission?.(opts)) === 'granted';
  }

  /** The folder's files as [path, File], skipping .git and build folders (and gen/, with `noGen`). */
  async *files(noGen = false) {
    async function* walk(dir, prefix) {
      for await (const [name, h] of dir.entries()) {
        const path = prefix + name;
        if (SKIP.test(path) || (noGen && path.startsWith('gen/'))) continue;
        if (h.kind === 'directory') yield* walk(h, `${path}/`);
        else yield [path, await h.getFile()];
      }
    }
    yield* walk(this.handle, '');
  }

  /** Every file in the folder (path -> text/bytes). */
  async read() {
    const files = {};
    for await (const [path, f] of this.files()) {
      this.stamps.set(path, f.lastModified);
      files[path] = new Uint8Array(await f.arrayBuffer());
    }
    return normalise(files);
  }

  /** The folder `path` is in (made, with `create`). */
  async dirOf(path, create = false) {
    let dir = this.handle;
    for (const p of path.split('/').slice(0, -1)) dir = await dir.getDirectoryHandle(p, { create });
    return dir;
  }

  /** Write `changed` (path -> text/bytes; null deletes) to the disk. */
  async write(changed) {
    for (const [path, data] of Object.entries(changed)) {
      if (data == null) {
        await this.delete(path);
        continue;
      }
      const h = await (await this.dirOf(path, true)).getFileHandle(path.split('/').at(-1), { create: true });
      const w = await h.createWritable();
      await w.write(typeof data === 'string' ? enc.encode(data) : data);
      await w.close();
      this.stamps.set(path, (await h.getFile()).lastModified);
    }
  }

  async delete(path) {
    try {
      await (await this.dirOf(path)).removeEntry(path.split('/').at(-1));
    } catch {}
    this.stamps.delete(path);
  }

  /**
   * Files changed on disk since they were last read or written (by your own
   * editor, git...): { path: text/bytes, gone: null }. Cheap enough to poll.
   */
  async changes() {
    const now = new Map();
    const changed = {};
    for await (const [path, f] of this.files(true)) {
      now.set(path, f.lastModified);
      if (this.stamps.get(path) === f.lastModified) continue;
      const bytes = new Uint8Array(await f.arrayBuffer());
      changed[path] = isText(path) ? new TextDecoder().decode(bytes) : bytes;
    }
    for (const p of this.stamps.keys()) if (!now.has(p) && !p.startsWith('gen/')) changed[p] = null;
    for (const [p, t] of now) this.stamps.set(p, t);
    for (const [p, d] of Object.entries(changed)) if (d === null) this.stamps.delete(p);
    return changed;
  }

  /**
   * Remember this folder under `key`, for the next visit (and the projects
   * menu). The same folder remembered before, under another key, is forgotten
   * there: returns the key it was under, or `key`.
   */
  async remember(key) {
    for (const old of await rememberedFolders()) {
      if (old.key !== key && (await old.handle.isSameEntry?.(this.handle))) await FolderStore.forget(old.key);
    }
    await tx('folders', 'readwrite', (s) => s.put(this.handle, key));
    return key;
  }

  /** The folder remembered under `key`, or null. */
  static async recall(key) {
    try {
      const h = await tx('folders', 'readonly', (s) => s.get(key));
      return h ? new FolderStore(h) : null;
    } catch {
      return null;
    }
  }

  static async forget(key) {
    await tx('folders', 'readwrite', (s) => s.delete(key));
  }

  /** Let the user pick a folder (`mode` readwrite); null if they cancel. */
  static async pick(id = 'breadbin-studio') {
    try {
      return new FolderStore(await globalThis.showDirectoryPicker({ id, mode: 'readwrite' }));
    } catch (e) {
      if (e.name === 'AbortError') return null;
      throw e;
    }
  }
}

/** The folders remembered (FolderStore.remember), newest first: [{ key, handle }]. */
export async function rememberedFolders() {
  try {
    const d = await db();
    return await new Promise((resolve, reject) => {
      const out = [];
      const req = d.transaction('folders', 'readonly').objectStore('folders').openCursor();
      req.onsuccess = () => {
        const c = req.result;
        if (!c) return resolve(out.sort((a, b) => b.at - a.at));
        out.push({ key: c.key, handle: c.value, at: Number(String(c.key).split(':').pop()) || 0 });
        c.continue();
      };
      req.onerror = () => reject(req.error);
    });
  } catch {
    return [];
  }
}

/** Is a folder empty enough to put a new project in (no project.json, few files)? */
export async function folderIsEmpty(store) {
  let n = 0;
  for await (const [name] of store.handle.entries()) {
    if (name === 'project.json') return false;
    if (!name.startsWith('.')) n++;
  }
  return n === 0;
}
