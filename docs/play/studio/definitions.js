// definitions - "go to definition" (F12, or Ctrl+click) in the code editor:
// where a name is defined among the project's C files. A function's body, a
// variable defined at file level (not an extern declaration), a #define, a
// struct, enum or typedef name, or a label in assembly. Prototypes count
// when nothing better is found (a runtime call: its declaration in bb.h).

const esc = (s) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

/**
 * Where `name` is defined in `files` (path -> text): { file, line } (1-based),
 * or null. `prefer` is searched first (the file being edited).
 */
export function findDefinition(files, name, prefer = '') {
  if (!/^[A-Za-z_]\w*$/.test(name)) return null;
  const n = esc(name);
  const tests = [
    // a function definition: the name, its arguments, no ';' after them on the line
    [/\.(c|h)$/, new RegExp(`^(?!\\s*(return|else|if|while|for|switch)\\b)[\\w\\s\\*]*\\b${n}\\s*\\([^;]*\\)\\s*\\{?\\s*(/[/*].*)?$`)],
    [/\.(c|h)$/, new RegExp(`^\\s*#\\s*define\\s+${n}\\b`)],
    [/\.(c|h)$/, new RegExp(`^\\s*(typedef\\b.*\\b${n}\\s*;|(struct|enum|union)\\s+${n}\\b|enum\\b.*\\{.*\\b${n}\\b)`)],
    // a variable at file level: a type, the name, maybe [size] or = value, and ';' or ','
    [/\.(c|h)$/, new RegExp(`^(?!\\s*(extern|return)\\b)(static\\s+|const\\s+|volatile\\s+|unsigned\\s+|signed\\s+)*\\w+[\\s\\*]+(\\w+\\s*(\\[[^\\]]*\\])?\\s*(=[^,;]*)?,\\s*)*${n}\\s*(\\[[^\\]]*\\])?\\s*(=|;|,)`)],
    [/\.(s|asm|a|inc)$/, new RegExp(`^${n}:|^${n}\\s*=|^\\s*\\.proc\\s+${n}\\b`)],
    // a prototype, at last: a type before the name (a call has none)
    [/\.(c|h)$/, new RegExp(`^(?!\\s*(return|if|else|while|for|do)\\b)\\s*(\\w+[\\s\\*]+)+${n}\\s*\\([^)]*\\)\\s*;`)],
  ];
  const order = [prefer, ...Object.keys(files).filter((f) => f !== prefer).sort((a, b) => rank(a) - rank(b) || a.localeCompare(b))].filter((f) => f in files && typeof files[f] === 'string');
  for (const [kind, re] of tests) {
    for (const file of order) {
      if (!kind.test(file)) continue;
      const lines = files[file].split('\n');
      let inComment = false;
      for (let i = 0; i < lines.length; i++) {
        const line = lines[i];
        // skip block comments, roughly
        if (inComment) {
          if (line.includes('*/')) inComment = false;
          continue;
        }
        if (/^\s*\/\*/.test(line) && !line.includes('*/')) {
          inComment = true;
          continue;
        }
        if (/^\s*(\/\/|\*)/.test(line)) continue;
        if (re.test(line)) return { file, line: i + 1 };
      }
    }
  }
  return null;
}

// the project's own sources first, then the generated ones (gen/: the runtime, the assets)
const rank = (f) => (/^src\//.test(f) ? 0 : /^gen\//.test(f) ? 2 : 1);
