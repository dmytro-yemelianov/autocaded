import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';
const page = readFileSync(new URL('../../web/index.html', import.meta.url), 'utf8');
const source = (name) => {
  const start = page.indexOf(`    function ${name}(`);
  const end = page.indexOf('\n    }', start) + '\n    }'.length;
  assert(start >= 0 && end > start);
  return page.slice(start, end);
};
class Select {
  value = '';
  options = [];
  listeners = new Map();
  option = {};
  blurred = false;
  contains(target) { return target === this || target === this.option; }
  querySelector(selector) { return this.options.find(o => selector === `option[value="${o.value}"]`); }
  appendChild(option) { this.options.push(option); }
  addEventListener(type, fn) { this.listeners.set(type, fn); }
  blur() { this.blurred = true; }
}
function setup() {
  const modeSelect = new Select();
  const paletteSelect = new Select();
  const localeSelect = new Select();
  const modeBadge = { dataset: {} };
  const modeText = {};
  const storage = new Map();
  let mode = 'frozen', palette = 'pc16', locale = 'en';
  let input = '1,2', submissions = 0;
  const calls = [], messages = [], handlers = new Map();
  const profiles = [
    { id: 'frozen', palette: 'pc16', tone: 'green', title: 'Rust faithful title', badge: 'Rust faithful badge', status: 'Rust faithful status' },
    { id: 'modern', palette: 'aci256', tone: 'red', title: 'Rust modern title', badge: 'Rust modern badge', status: 'Rust modern status' },
  ];
  const context = vm.createContext({
    modeSelect, paletteSelect, modeBadge, modeText, modeDot: { style: {} }, profileMessage: false, messageEl: {},
    document: { createElement: () => ({}), getElementById: id => ({'mode-select': modeSelect, 'palette-select': paletteSelect, 'locale-select': localeSelect})[id] || {} },
    localStorage: { setItem: (k, v) => storage.set(k, v), getItem: k => storage.get(k), removeItem: k => storage.delete(k) },
    cad: {
      set_mode(id) { calls.push(['mode', id]); const p = profiles.find(p => p.id === id); if (!p) throw Error('invalid profile'); mode = id; palette = p.palette; },
      mode: () => mode,
      set_palette(name) { calls.push(['palette', name]); palette = name; },
      presentation_profiles: () => JSON.stringify({ schema_version: 1, mode, palette, profiles: profiles.map(p => ({ ...p, title: locale === 'uk' ? 'Каталог ' + p.title : p.title, status: locale === 'uk' ? 'Режим ' + p.status : p.status })) }),
      key_down(key) { if (key === 'Enter') submissions++; else input += key; },
    },
    message: (...args) => messages.push(args), redraw() {},
    window: { addEventListener: (event, fn) => handlers.set(event, fn) },
    HTMLSelectElement: Select, aboutModal: { hidden: true },
  });
  for (const name of ['refreshModeUi', 'applyMode', 'changeMode', 'applyPalette', 'restorePresentationPreferences']) vm.runInContext(source(name), context);
  return { context, modeSelect, paletteSelect, localeSelect, modeBadge, modeText, storage, calls, profiles, messages, handlers,
    get input() { return input; }, get submissions() { return submissions; }, setLocale(tag) { locale = tag; } };
}
test('actual profile handler delegates once, resolves labels, preserves ID across manual palette and locale', () => {
  const s = setup(); s.storage.set('autocaded.palette', 'pc16');
  vm.runInContext("applyMode('modern')", s.context);
  assert.deepEqual(s.calls, [['mode', 'modern']]);
  assert.equal(s.modeSelect.value, 'modern'); assert.equal(s.paletteSelect.value, 'aci256');
  assert.equal(s.modeText.textContent, 'Rust modern badge');
  assert.equal(s.context.modeDot.style.background, 'var(--accent-red)');
  assert.deepEqual(s.messages, [['Rust modern status']]);
  assert.equal(s.storage.get('autocaded.mode'), 'modern'); assert(!s.storage.has('autocaded.palette'));
  vm.runInContext("applyPalette('pc16'); refreshModeUi()", s.context);
  assert.equal(s.modeSelect.value, 'modern'); assert.equal(s.paletteSelect.value, 'pc16');
  s.setLocale('uk'); vm.runInContext('refreshModeUi()', s.context);
  assert.equal(s.modeSelect.options.length, 2);
  assert.equal(s.modeSelect.options[0].textContent, 'Каталог Rust faithful title');
  assert.equal(s.modeSelect.value, 'modern'); assert.equal(s.paletteSelect.value, 'pc16');
  assert.equal(s.context.messageEl.textContent, 'Режим Rust modern status');
  assert.equal(s.input, '1,2'); assert.equal(s.submissions, 0);
});
test('startup restores preset before separate manual override; rejected profile never persists', () => {
  const s = setup();
  s.storage.set('autocaded.mode', 'modern'); s.storage.set('autocaded.palette', 'pc16');
  vm.runInContext('restorePresentationPreferences(); refreshModeUi()', s.context);
  assert.deepEqual(s.calls, [['mode', 'modern'], ['palette', 'pc16']]);
  assert.equal(s.modeSelect.value, 'modern'); assert.equal(s.paletteSelect.value, 'pc16');
  vm.runInContext("applyMode('future'); refreshModeUi()", s.context);
  assert.equal(s.storage.get('autocaded.mode'), 'modern'); assert.equal(s.storage.get('autocaded.palette'), 'pc16');
  assert.equal(s.modeSelect.value, 'modern'); assert.equal(s.paletteSelect.value, 'pc16');
});
test('profile and palette chooser own keys and actual change handler never submits input', () => {
  const s = setup();
  const start = page.indexOf("    const softKeys = document.getElementById('soft-keys');");
  const end = page.indexOf('\n    // A saved view', start);
  vm.runInContext(page.slice(start, end), s.context);
  const keydown = s.handlers.get('keydown');
  for (const select of [s.modeSelect, s.paletteSelect]) for (const target of [select, select.option]) {
    for (const key of ['m', 'ArrowDown', 'Enter', ' ', 'Escape', 'c']) {
      const event = {target, key, ctrlKey:key === 'c', defaultPrevented:false, preventDefault() {this.defaultPrevented = true;} };
      keydown(event); assert.equal(event.defaultPrevented, false); assert.equal(select.blurred, false);
    }
  }
  const registration = "    modeSelect.addEventListener('change', changeMode);";
  assert(page.includes(registration)); vm.runInContext(registration, s.context);
  s.modeSelect.value = 'modern'; s.modeSelect.listeners.get('change')({target:s.modeSelect});
  assert.equal(s.input, '1,2'); assert.equal(s.submissions, 0); assert.equal(s.modeSelect.value, 'modern');
});
