// Run the /play/ bridge, not a source-text approximation of its cfg arm.
import {readFileSync} from 'node:fs';
import assert from 'node:assert/strict';
const module = await WebAssembly.compile(readFileSync(process.argv[2]));
assert.deepEqual(WebAssembly.Module.imports(module), [], 'the current bridge has no host entropy shim');
const {exports: e} = await WebAssembly.instantiate(module, {});
for (const file of process.argv.slice(3)) {
  const source = readFileSync(file);
  const ptr = e.lupin_alloc(source.length);
  new Uint8Array(e.memory.buffer, ptr, source.length).set(source);
  const result = e.lupin_record(ptr, source.length);
  const len = new DataView(e.memory.buffer).getUint32(result, true);
  const text = new TextDecoder().decode(new Uint8Array(e.memory.buffer, result + 4, len));
  const r = JSON.parse(JSON.parse(text).record);
  console.log(JSON.stringify(r));
  e.lupin_result_free(result);
  e.lupin_free(ptr, source.length);
  assert.equal(r.verdict, 'unsupported');
  assert.match(r['x-unsupported'], /`os_random`/);
  assert.doesNotMatch(r['x-unsupported'], /does not resolve/);
  assert.match(r['x-unsupported'], /wasm.*tier|tier.*wasm/);
}
