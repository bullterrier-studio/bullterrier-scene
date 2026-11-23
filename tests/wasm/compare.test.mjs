// The package in Node renders what the Python package renders: the same size and pixels that look the same
// (looks_the_same.mjs; never deepEqual on a big buffer: a mismatch builds a diff of the whole value). oracle.py wrote the
// answers and the files each scene names, which the package is handed as bytes: a browser has no folder to read.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { test } from "node:test";

import { differenceInLooks } from "./looks_the_same.mjs";

const { Scene } = createRequire(import.meta.url)("../../pkg/bullterrier_scene_wasm.js");
const oracle = new URL("../../pkg/oracle/", import.meta.url);
const answers = JSON.parse(readFileSync(new URL("oracle.json", oracle)));

for (const [name, expected] of Object.entries(answers)) {
  test(`${name}: pixels that look the same as the Python package's render`, () => {
    const files = Object.fromEntries(expected.files.map((path) => [path, readFileSync(new URL(path, oracle))]));
    const scene = Scene.fromJson(readFileSync(new URL(`cases/${expected.scene}`, import.meta.url), "utf8"), { files });
    const out = expected.size === null ? scene.render() : scene.render({ size: expected.size });
    assert.deepEqual([out.width, out.height], [expected.width, expected.height]);
    assert.ok(out.data instanceof Uint8ClampedArray);
    assert.equal(differenceInLooks(out.data, readFileSync(new URL(`${name}.rgba`, oracle))), null);
  });
}
