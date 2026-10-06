import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, writeFileSync, readdirSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, basename } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const root = fileURLToPath(new URL('../../', import.meta.url));
test('demo staging includes canonical editable artwork and rejects duplicate staging before writes', () => {
  const dir = mkdtempSync(join(tmpdir(), 'autorust-art-menu-'));
  try {
    const manifest = join(dir, 'manifest.json');
    writeFileSync(manifest, JSON.stringify({ samples: [{ file: 'WELCOME.DWG', label: 'Welcome' }] }));
    const run = () => spawnSync('python3', [join(root, 'tools/demo/stage_art.py'), manifest, dir], { encoding: 'utf8' });
    const result = run();
    assert.equal(result.status, 0, result.stderr);
    const samples = JSON.parse(readFileSync(manifest)).samples;
    const catalog = JSON.parse(readFileSync(join(root, 'demo/art/catalog.json')));
    assert.equal(samples.length, catalog.entries.length + 1);
    for (const entry of catalog.entries) {
      const file = basename(entry.outputs.dwg);
      assert.equal(samples.find(s => s.file === file).label_key, entry.title_key);
      assert.deepEqual(readFileSync(join(dir, file)), readFileSync(join(root, 'demo/art', entry.outputs.dwg)));
    }
    const before = readFileSync(manifest);
    const files = readdirSync(dir);
    assert.notEqual(run().status, 0);
    assert.deepEqual(readFileSync(manifest), before);
    assert.deepEqual(readdirSync(dir), files);
  } finally {
    rmSync(dir, { recursive: true });
  }
});
