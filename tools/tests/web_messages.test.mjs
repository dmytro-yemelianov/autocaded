import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';

const page = readFileSync(new URL('../../web/index.html', import.meta.url), 'utf8');
const functionSource = (name) => {
  const start = page.indexOf(`    function ${name}(`);
  const end = page.indexOf('\n    }', start) + '\n    }'.length;
  assert(start >= 0 && end > start);
  return page.slice(start, end);
};

test('wheel/button zoom uses semantic idle, retaining input and report guards', () => {
  for (const [state, expected] of [
    [{ command_idle: true, prompt: 'Команда', input: '', report_visible: false }, ['ZOOM', '1.2X']],
    [{ command_idle: false, prompt: 'Command', input: '', report_visible: false }, []],
    [{ command_idle: true, prompt: 'Команда', input: 'L', report_visible: false }, []],
    [{ command_idle: true, prompt: 'Команда', input: '', report_visible: true }, []],
  ]) {
    const commands = [];
    vm.runInNewContext(`${functionSource('zoom')}\nzoom(1.2);`, {
      cad: { get_state_json: () => JSON.stringify(state), command: (line) => commands.push(line) },
      redraw() {},
    });
    assert.deepEqual(commands, expected);
  }
});

test('picker labels use Rust resolved keys as text and refresh on redraw', () => {
  const options = { '#open-select option[value=""]': {}, '#save-select option[value=""]': {} };
  let labels = { 'ui.file.open_picker': 'Open…', 'ui.file.save_picker': 'Save as…' };
  const context = vm.createContext({
    cad: { locale: () => 'en', ui_labels: () => JSON.stringify({ schema_version: 1, labels }),
      render_rgba: () => new Uint8Array(4) },
    document: { getElementById: () => ({}), querySelector: (selector) => options[selector] },
    canvas: { width: 1, height: 1 }, ctx: { putImageData() {} }, console,
    ImageData: class { constructor() {} }, Uint8ClampedArray, refreshModeUi() {}, refreshSampleLabels() {},
  });
  vm.runInContext(functionSource('redraw'), context);
  vm.runInContext('redraw()', context);
  assert.equal(options['#open-select option[value=""]'].textContent, 'Open…');
  assert.equal(options['#save-select option[value=""]'].textContent, 'Save as…');
  labels = { 'ui.file.open_picker': '<literal label>', 'ui.file.save_picker': '{literal label}' };
  vm.runInContext('redraw()', context);
  assert.equal(options['#open-select option[value=""]'].textContent, '<literal label>');
  assert.equal(options['#save-select option[value=""]'].textContent, '{literal label}');
  assert(!page.includes('<option value="">Open…</option>'));
  assert(!page.includes('<option value="">Save as…</option>'));
});

test('locale chooser delegates to Session and restores actual state on rejection or fallback', () => {
  assert(page.includes('Українська (частково)'));
  for (const tag of ['uk', 'en', 'fr-CA', 'uk_UA']) {
    let locale = 'en';
    const errors = [];
    let redraws = 0;
    const event = { target: { value: tag } };
    vm.runInNewContext(functionSource('changeLocale') + '\nchangeLocale(event);', {
      event,
      cad: {
        set_locale(value) {
          assert.equal(value, tag);
          if (value === 'uk_UA') throw new Error('malformed locale tag');
          locale = value === 'uk' ? 'uk' : 'en';
        },
        locale: () => locale,
      },
      message: (text, isError) => errors.push([text, isError]),
      redraw: () => redraws++,
    });
    assert.equal(event.target.value, tag === 'uk' ? 'uk' : 'en');
    assert.equal(errors.length, tag === 'uk_UA' ? 1 : 0);
    assert.equal(redraws, 1);
  }
});


test('locale chooser retains default English before Session is ready', () => {
  const event = { target: { value: 'uk' } };
  vm.runInNewContext(functionSource('changeLocale') + '\nchangeLocale(event);', { event, cad: null });
  assert.equal(event.target.value, 'en');
});


test('focused locale select owns keys and change without editing or submitting CAD input', () => {
  class Select {
    value = 'en';
    blurred = false;
    listeners = new Map();
    constructor() { this.option = {}; }
    contains(target) { return target === this || target === this.option; }
    blur() { this.blurred = true; }
    addEventListener(type, handler) { this.listeners.set(type, handler); }
  }
  const select = new Select();
  const otherSelect = new Select();
  const softKeys = {};
  const handlers = new Map();
  let input = '1,2';
  let submissions = 0;
  let locale = 'en';
  const context = vm.createContext({
    document: { getElementById: (id) => id === 'locale-select' ? select : id === 'soft-keys' ? softKeys : { contains: () => false } },
    window: { addEventListener: (type, handler) => handlers.set(type, handler) },
    HTMLSelectElement: Select,
    aboutModal: { hidden: true },
    cad: {
      key_down(key) {
        if (key === 'Enter') submissions++;
        else input += key;
        return true;
      },
      set_locale: (tag) => { locale = tag; },
      locale: () => locale,
    },
    redraw() {},
    message() {},
  });
  vm.runInContext(functionSource('changeLocale'), context);
  const registration = "    document.getElementById('locale-select').addEventListener('change', changeLocale);";
  assert(page.includes(registration));
  vm.runInContext(registration, context);
  const start = page.indexOf("    const softKeys = document.getElementById('soft-keys');");
  const end = page.indexOf("\n    // A saved view", start);
  assert(start >= 0 && end > start);
  vm.runInContext(page.slice(start, end), context);
  const keydown = handlers.get('keydown');
  for (const target of [select, select.option]) {
    for (const key of ['u', 'ArrowDown', 'Enter', ' ', 'Escape', 'c']) {
      const event = { target, key, ctrlKey: key === 'c', defaultPrevented: false,
        preventDefault() { this.defaultPrevented = true; } };
      keydown(event);
      assert.equal(event.defaultPrevented, false, 'native select handling stays available');
      assert.equal(select.blurred, false);
      assert.equal(input, '1,2');
      assert.equal(submissions, 0);
    }
  }
  select.value = 'uk';
  select.listeners.get('change')({ target: select });
  assert.equal(locale, 'uk');
  assert.equal(select.value, 'uk');
  assert.equal(input, '1,2');
  assert.equal(submissions, 0);
  const event = { target: otherSelect, key: 'x', preventDefault() {} };
  keydown(event);
  assert.equal(otherSelect.blurred, true, 'existing selector routing remains unchanged');
  assert.equal(input, '1,2x');
});


test('project link keyboard activation stays outside CAD after integration', () => {
  const start = page.indexOf("    window.addEventListener('keydown', (e) => {");
  const end = page.indexOf('\n    });', start) + '\n    });'.length;
  assert(start >= 0 && end > start);
  let handler;
  vm.runInNewContext(page.slice(start, end), {
    window: { addEventListener: (_type, callback) => { handler = callback; } },
    cad: { key_down() { assert.fail('link activation reached CAD'); } },
    aboutModal: { hidden: true }, softKeys: {},
  });
  for (const key of ['Enter', ' ', 'u']) {
    handler({ key, target: { closest: (selector) => selector === '.project-links' ? {} : null },
      preventDefault() { assert.fail('link activation was prevented'); } });
  }
});

test('artwork sample titles refresh from shared locale keys with literal fallback', () => {
  const option = { dataset: { labelKey: 'art.courtyard_house.title', fallbackLabel: 'Courtyard house' } };
  const context = vm.createContext({ document: { querySelectorAll: () => [option] }, labels: {} });
  vm.runInContext(functionSource('refreshSampleLabels'), context);
  vm.runInContext('refreshSampleLabels(labels)', context);
  assert.equal(option.textContent, 'Courtyard house');
  context.labels = { 'art.courtyard_house.title': 'Будинок із подвір’ям' };
  vm.runInContext('refreshSampleLabels(labels)', context);
  assert.equal(option.textContent, 'Будинок із подвір’ям');
  context.labels = { 'art.courtyard_house.title': '<literal title>' };
  vm.runInContext('refreshSampleLabels(labels)', context);
  assert.equal(option.textContent, '<literal title>');
});
