import test from 'node:test';
import assert from 'node:assert/strict';
import { installCanvasInput, installModalKeyboard } from '../../web/input.mjs';

class Surface {
  width = 800;
  height = 600;
  listeners = new Map();
  addEventListener(type, listener, capture = false) {
    const list = this.listeners.get(type) || [];
    list.push({ listener, capture });
    this.listeners.set(type, list);
  }
  setPointerCapture() {}
  getBoundingClientRect() { return { left: 0, top: 0, width: 400, height: 300 }; }
  emit(type, values = {}) {
    const event = { button: 0, pointerType: 'touch', pointerId: 1,
      clientX: 100, clientY: 100, stopped: false, defaultPrevented: false,
      stopPropagation() { this.stopped = true; },
      preventDefault() { this.defaultPrevented = true; }, ...values };
    for (const { listener } of this.listeners.get(type) || []) listener(event);
    return event;
  }
}

function setup(state = {}) {
  const canvas = new Surface();
  const calls = [];
  const cadState = { prompt: 'Command', input: '', report_visible: false, sketch_active: false, ...state };
  const cad = { get_state_json: () => JSON.stringify(cadState) };
  for (const method of ['press', 'release', 'motion', 'cursor', 'pan']) {
    cad[method] = (...args) => calls.push([method, ...args]);
  }
  installCanvasInput(canvas, { session: () => cad, redraw() {},
    zoom: (factor) => calls.push(['zoom', factor]), onError: (error) => { throw error; } });
  return { canvas, calls, cadState };
}

test('mouse presses and sketch releases retain the native route', () => {
  const { canvas, calls } = setup();
  const mouse = { pointerType: 'mouse' };
  canvas.emit('pointerdown', mouse);
  canvas.emit('pointermove', { ...mouse, clientX: 110 });
  canvas.emit('pointerup', { ...mouse, clientX: 110 });
  assert.deepEqual(calls.map(([method]) => method), ['press', 'motion', 'cursor', 'release']);
});

test('a touch tap commits one point only after release', () => {
  const { canvas, calls } = setup({ prompt: 'LINE: start point' });
  canvas.emit('pointerdown');
  canvas.emit('pointermove', { clientX: 102 });
  assert.deepEqual(calls, []);
  canvas.emit('pointerup', { clientX: 102 });
  assert.deepEqual(calls, [['press', 204, 200, 800, 600], ['release', 204, 200, 800, 600]]);
});

test('two fingers during LINE never place points, including the remaining finger', () => {
  const { canvas, calls } = setup({ prompt: 'LINE: start point' });
  canvas.emit('pointerdown');
  canvas.emit('pointerdown', { pointerId: 2, clientX: 200 });
  canvas.emit('pointermove', { pointerId: 2, clientX: 220 });
  canvas.emit('pointerup', { pointerId: 2, clientX: 220 });
  canvas.emit('pointermove', { clientX: 110 });
  canvas.emit('pointerup', { clientX: 110 });
  assert.deepEqual(calls, []);
});

test('an idle drag pans in physical pixels without committing a click', () => {
  const { canvas, calls } = setup();
  canvas.emit('pointerdown');
  canvas.emit('pointermove', { clientX: 102 });
  canvas.emit('pointermove', { clientX: 140, clientY: 130 });
  canvas.emit('pointerup', { clientX: 140, clientY: 130 });
  assert.deepEqual(calls, [['pan', 80, 60, 800, 600], ['cursor', 280, 260]]);
});

test('pinching navigates with the measured scale and centroid displacement', () => {
  const { canvas, calls } = setup();
  canvas.emit('pointerdown');
  canvas.emit('pointerdown', { pointerId: 2, clientX: 200 });
  canvas.emit('pointermove', { pointerId: 2, clientX: 220 });
  canvas.emit('pointerup', { pointerId: 2, clientX: 220 });
  canvas.emit('pointerup');
  assert.deepEqual(calls, [['pan', 20, 0, 800, 600], ['zoom', 1.2]]);
});

test('navigation does not consume typed input or an open report', () => {
  for (const state of [{ input: 'LI' }, { report_visible: true }]) {
    const { canvas, calls } = setup(state);
    canvas.emit('pointerdown');
    canvas.emit('pointerdown', { pointerId: 2, clientX: 200 });
    canvas.emit('pointermove', { pointerId: 2, clientX: 220 });
    canvas.emit('pointerup', { pointerId: 2 });
    canvas.emit('pointerup');
    assert.deepEqual(calls, []);
  }
});

test('sketch drags start at the initial contact and lift on cancellation', () => {
  const { canvas, calls } = setup({ prompt: 'SKETCH', sketch_active: true });
  canvas.emit('pointerdown');
  assert.deepEqual(calls, []);
  canvas.emit('pointermove', { clientX: 110 });
  canvas.emit('pointercancel', { clientX: 110 });
  assert.deepEqual(calls, [['press', 200, 200, 800, 600], ['motion', 220, 200, 800, 600],
    ['cursor', 220, 200], ['release', 220, 200, 800, 600]]);
});

test('a second finger lifts an existing sketch drag and suppresses subsequent points', () => {
  const { canvas, calls } = setup({ prompt: 'SKETCH', sketch_active: true });
  canvas.emit('pointerdown');
  canvas.emit('pointermove', { clientX: 110 });
  canvas.emit('pointerdown', { pointerId: 2, clientX: 200 });
  canvas.emit('pointerup', { pointerId: 2 });
  canvas.emit('pointerup', { clientX: 110 });
  assert.deepEqual(calls.map(([method]) => method), ['press', 'motion', 'cursor', 'release']);
});

test('cancelled taps place no point and the next tap works', () => {
  const { canvas, calls } = setup({ prompt: 'LINE: start point' });
  canvas.emit('pointerdown');
  canvas.emit('pointercancel');
  assert.deepEqual(calls, []);
  canvas.emit('pointerdown');
  canvas.emit('pointerup');
  assert.deepEqual(calls.map(([method]) => method), ['press', 'release']);
});

test('modal keys stop before CAD handlers, and Escape closes without cancelling CAD', () => {
  const document = new Surface();
  const modal = { hidden: false };
  installModalKeyboard(document, modal, () => { modal.hidden = true; });
  assert.equal(document.listeners.get('keydown')[0].capture, true);
  for (const key of ['x', 'Enter', 'Escape']) {
    const event = document.emit('keydown', { key });
    assert.equal(event.stopped, true);
    assert.equal(event.defaultPrevented, key === 'Escape');
  }
  assert.equal(modal.hidden, true);
  assert.equal(document.emit('keydown', { key: 'x' }).stopped, false);
});
