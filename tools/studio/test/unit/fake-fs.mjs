// An in-memory stand-in for the File System Access API's directory handles,
// enough of it for FolderStore: entries(), get*Handle(), removeEntry(),
// getFile() with lastModified, createWritable().

let clock = 1_000_000;

export class FakeDir {
  constructor(name = 'project') {
    this.kind = 'directory';
    this.name = name;
    this.children = new Map();
  }
  async *entries() {
    for (const [k, v] of this.children) yield [k, v];
  }
  async getDirectoryHandle(name, { create = false } = {}) {
    let d = this.children.get(name);
    if (!d) {
      if (!create) throw Object.assign(new Error(`${name} not found`), { name: 'NotFoundError' });
      this.children.set(name, (d = new FakeDir(name)));
    }
    return d;
  }
  async getFileHandle(name, { create = false } = {}) {
    let f = this.children.get(name);
    if (!f) {
      if (!create) throw Object.assign(new Error(`${name} not found`), { name: 'NotFoundError' });
      this.children.set(name, (f = new FakeFile(name)));
    }
    return f;
  }
  async removeEntry(name) {
    if (!this.children.delete(name)) throw new Error(`${name} not found`);
  }
  async queryPermission() {
    return 'granted';
  }
  /** Test helper: put `text` at `path`, as another program would. */
  async put(path, text) {
    const parts = path.split('/');
    let d = this;
    for (const p of parts.slice(0, -1)) d = await d.getDirectoryHandle(p, { create: true });
    const f = await d.getFileHandle(parts.at(-1), { create: true });
    f.data = new TextEncoder().encode(text);
    f.modified = ++clock;
  }
}

export class FakeFile {
  constructor(name) {
    this.kind = 'file';
    this.name = name;
    this.data = new Uint8Array();
    this.modified = ++clock;
  }
  async getFile() {
    const data = this.data;
    return { lastModified: this.modified, arrayBuffer: async () => data.slice().buffer };
  }
  async createWritable() {
    const chunks = [];
    return {
      write: async (d) => chunks.push(typeof d === 'string' ? new TextEncoder().encode(d) : d),
      close: async () => {
        const n = chunks.reduce((s, c) => s + c.length, 0);
        this.data = new Uint8Array(n);
        let at = 0;
        for (const c of chunks) (this.data.set(c, at), (at += c.length));
        this.modified = ++clock;
      },
    };
  }
}
