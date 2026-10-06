import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { runInNewContext } from 'node:vm';

const source = readFileSync(new URL('../../web/sw.js', import.meta.url), 'utf8');

function install(missing) {
  const handlers = new Map();
  let activated = false;
  const cacheNames = new Set(['autocaded-v2']);
  runInNewContext(source, {
    Request: class { constructor(url) { this.url = url; } },
    fetch: async () => { throw new Error('No optional demo manifest'); },
    caches: {
      open: async name => {
        cacheNames.add(name);
        return { add: async request => {
          if (request.url === missing) throw new Error(`Missing ${missing}`);
        } };
      },
    },
    self: {
      addEventListener: (name, handler) => handlers.set(name, handler),
      skipWaiting: async () => { activated = true; },
    },
  });
  let pending;
  handlers.get('install')({ waitUntil: value => { pending = value; } });
  return { pending, cacheNames, activated: () => activated };
}

test('missing required editor assets prevent replacing the working offline worker', async () => {
  for (const missing of ['./', './index.html', './input.mjs', './pkg/acad_wasm.js', './pkg/acad_wasm_bg.wasm']) {
    const attempt = install(missing);
    await assert.rejects(attempt.pending, /Missing/);
    assert.equal(attempt.activated(), false);
    assert.equal(attempt.cacheNames.has('autocaded-v2'), true);
  }
});

test('a missing optional icon does not block the new offline worker', async () => {
  const attempt = install('./icon-192.png');
  await attempt.pending;
  assert.equal(attempt.activated(), true);
});
