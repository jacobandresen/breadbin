// ESLint for breadbin's JavaScript: mistakes (eslint's recommended rules) and
// the measures tools/quality.mjs tracks (function length, nesting, branches).
// ESLint itself is a dev dependency of tools/studio: `make quality` runs it.
import js from './tools/studio/node_modules/@eslint/js/src/index.js';
import globals from './tools/studio/node_modules/globals/index.js';

// vc64web's page functions, called by the scripts breadbin runs in it (emu.js, drive.js, the tests)
const VC64WEB = Object.fromEntries([
  'wasm_peek', 'wasm_poke', 'wasm_auto_type', 'wasm_key', 'wasm_sprite_info', 'wasm_get_config', 'wasm_configure',
  'wasm_retro_shell_get_text', 'wasm_retro_shell_press_key', 'wasm_retro_shell_press_special', 'wasm_get_cpu_cycles',
  'wasm_halt', 'wasm_run', 'wasm_reset', 'wasm_set_warp', 'wasm_take_user_snapshot', 'wasm_loadfile', 'wasm_schedule_key',
  'emit_joystick_cmd', 'floppy_has_disk', 'file_slot_file_name', 'local_storage_get', 'Module',
].map((n) => [n, 'readonly']));

export default [
  {
    ignores: [
      'docs/play/acme.mjs', // ACME, compiled by emscripten
      'docs/play/studio/vendor/**', // the CodeMirror bundle
      'docs/play/studio/cc65/**', // cc65, compiled by emscripten
      'docs/play/studio/examples/**',
      'tools/studio/node_modules/**',
      'tools/studio/examples/**',
      'target/**',
    ],
  },
  js.configs.recommended,
  {
    files: ['**/*.js', '**/*.mjs'],
    languageOptions: {
      ecmaVersion: 2024,
      sourceType: 'module',
      globals: { ...globals.browser, ...globals.node, ...globals.worker, ...globals.audioWorklet, ...VC64WEB },
    },
    rules: {
      'no-unused-vars': ['error', { args: 'after-used', caughtErrors: 'none', ignoreRestSiblings: true }],
      'no-empty': ['error', { allowEmptyCatch: true }],
      // the measures: warnings, counted by tools/quality.mjs
      complexity: ['warn', 15],
      'max-depth': ['warn', 4],
      'max-lines-per-function': ['warn', { max: 60, skipBlankLines: true, skipComments: true }],
      'max-params': ['warn', 5],
    },
  },
];
