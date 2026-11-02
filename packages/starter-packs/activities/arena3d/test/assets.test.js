// @ts-check
// assets/: the glTF loader on the bundled sample model, the bundled source's path rules, and the shelf stub.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { flattenScene, loadGltf, parseDataUri } from '../assets/gltf.js';
import { AssetNotAvailableError, BundledAssetSource, ShelfAssetSource, assetPathProblem } from '../assets/source.js';
import { FLOATS_PER_VERTEX } from '../render/meshes.js';
import { mat4 } from '../render/math.js';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const beacon = JSON.parse(readFileSync(join(ROOT, 'assets', 'models', 'beacon.json'), 'utf8'));

test('the sample model loads: meshes, hierarchy, materials and its embedded PNG', async () => {
  /** @type {{ mime: string, n: number }[]} */
  const decoded = [];
  const model = await loadGltf(beacon, {
    decodeImage: async (bytes, mime) => {
      decoded.push({ mime, n: bytes.length });
      assert.deepEqual([...bytes.subarray(0, 4)], [137, 80, 78, 71], 'a PNG signature');
      return /** @type {any} */ ({ width: 16, height: 16 });
    },
  });
  assert.deepEqual(model.meshes.map((m) => m.name), ['Pedestal', 'Crystal']);
  assert.deepEqual(decoded, [{ mime: 'image/png', n: decoded[0].n }]);
  assert.equal(model.images.length, 1);
  assert.equal(model.materials[1].baseColorImage, 0);
  assert.deepEqual(model.materials[0].baseColor, [1, 1, 1, 1]);
  // Pedestal: 8 sides x 2 triangles + 8 top triangles, flat shaded (no NORMAL in the file), coloured.
  const ped = model.meshes[0].primitives[0].mesh;
  assert.equal(ped.indices.length, 24 * 3);
  assert.equal(ped.vertices.length, 24 * 3 * FLOATS_PER_VERTEX);
  const topNormalY = ped.vertices[(16 * 3) * FLOATS_PER_VERTEX + 4];
  assert.ok(topNormalY > 0.99 || ped.vertices.some((_, i) => i % FLOATS_PER_VERTEX === 4 && ped.vertices[i] > 0.99), 'some faces point up');
  // Crystal: 8 faces with the file's normals and UVs.
  const cr = model.meshes[1].primitives[0].mesh;
  assert.equal(cr.indices.length, 24);
  // The scene: the crystal sits 1.25 m above the root, scaled 0.6 in y.
  const flatList = flattenScene(model, mat4.fromTRS([10, 0, 0], [0, 0, 0, 1], [1, 1, 1]));
  assert.equal(flatList.length, 2);
  const crystal = flatList.find((f) => f.node === 2);
  assert.ok(crystal);
  const top = mat4.transformPoint(crystal.world, [0, 1, 0]);
  assert.ok(Math.abs(top[0] - 10) < 1e-5 && Math.abs(top[1] - 1.85) < 1e-5, `crystal tip at ${top}`);
});

test('a document that is not glTF 2.0, or that uses a .glb chunk, is refused', async () => {
  await assert.rejects(loadGltf({ asset: { version: '1.0' } }), /glTF 2.0/);
  await assert.rejects(loadGltf({ asset: { version: '2.0' }, buffers: [{ byteLength: 4 }] }), /data: URI/);
});

test('external buffers go through the asset source, relative to the model folder', async () => {
  const doc = structuredClone(beacon);
  const data = parseDataUri(doc.buffers[0].uri);
  assert.ok(data);
  doc.buffers[0].uri = 'beacon.bin.txt';
  /** @type {string[]} */
  const asked = [];
  const source = { kind: 'test', bytes: async (/** @type {string} */ p) => (asked.push(p), data.bytes), json: async () => ({}), image: async () => /** @type {any} */ (null) };
  const model = await loadGltf(doc, { source, basePath: 'models', decodeImage: null });
  assert.deepEqual(asked, ['models/beacon.bin.txt']);
  assert.equal(model.meshes.length, 2);
  assert.deepEqual(model.images, [null], 'decodeImage: null skips images');
});

test('the bundled source resolves relative paths only', async () => {
  for (const bad of ['', '/etc/passwd', '../x.json', 'a/../b.json', 'https://example.invalid/x.json', '//cdn/x', 'a\\b', 'data:x']) assert.ok(assetPathProblem(bad), bad);
  assert.equal(assetPathProblem('models/beacon.json'), null);
  /** @type {string[]} */
  const fetched = [];
  const src = new BundledAssetSource({
    base: pathToFileURL(join(ROOT, 'assets') + '/'),
    doc: null,
    fetch: /** @type {any} */ (async (/** @type {string} */ url) => {
      fetched.push(url);
      return new Response(readFileSync(fileURLToPath(url)));
    }),
  });
  const json = await src.json('models/beacon.json');
  assert.equal(json.asset.version, '2.0');
  assert.ok(fetched[0].endsWith('/assets/models/beacon.json'));
  await assert.rejects(src.json('../package.json'), /\.\./);
});

test('the shelf source is a stub that says it is not available yet', async () => {
  const shelf = new ShelfAssetSource();
  assert.equal(ShelfAssetSource.available, false);
  for (const call of [() => shelf.bytes('x.glb'), () => shelf.json('x.gltf'), () => shelf.image('x.ktx2')]) {
    await assert.rejects(call(), (e) => e instanceof AssetNotAvailableError && /not available yet/.test(e.message));
  }
});
