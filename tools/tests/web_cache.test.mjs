import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';

const worker = readFileSync(new URL('../../web/sw.js', import.meta.url), 'utf8');

test('network-first assets revalidate the browser cache and still work offline', async () => {
  const handlers = new Map();
  const old = { version: 'old' };
  const current = { version: 'new', ok: true, clone() { return this; } };
  let offline = false;
  const stored = new Map();
  const request = { method: 'GET', url: 'https://example.test/pkg/acad_wasm.js' };
  vm.runInNewContext(worker, {
    self: { location: { origin: 'https://example.test' },
      addEventListener: (name, handler) => handlers.set(name, handler) },
    URL,
    caches: { open: async () => ({
      put: async (key, value) => stored.set(key.url, value),
      match: async (key) => stored.get(key.url),
    }) },
    fetch: async (_request, options) => {
      if (offline) throw new Error('offline');
      // Model an HTTP cache that still regards the previous build as fresh.
      return options?.cache === 'no-cache' ? current : old;
    },
  });
  const load = () => new Promise((resolve, reject) => {
    handlers.get('fetch')({ request,
      respondWith: (response) => response.then(resolve, reject) });
  });
  assert.equal((await load()).version, 'new');
  offline = true;
  assert.equal((await load()).version, 'new');
});
