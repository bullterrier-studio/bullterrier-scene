// The package's interface: what a page building controls for a poster and showing its errors relies on.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { test } from "node:test";

import { differenceInLooks } from "./looks_the_same.mjs";

const { Scene } = createRequire(import.meta.url)("../../pkg/bullterrier_scene_wasm.js");
const oracle = new URL("../../pkg/oracle/", import.meta.url);
const masked = readFileSync(new URL("cases/masked-picture.json", import.meta.url), "utf8");
const maskedFiles = () => ({
  "photo.png": readFileSync(new URL("photo.png", oracle)),
  "maps/matte.png": readFileSync(new URL("maps/matte.png", oracle)),
});

// a ground with a look that has a colour, and a title in a bundled font: no file to hand in
const poster = JSON.stringify({
  version: 1,
  canvas: { width: 120, height: 80 },
  layers: [
    { name: "ground", fill: "#204060", looks: [{ recipe: { steps: [{ effect: "light_leak" }] } }] },
    { name: "title", text: "Hi", font: "bricolage_grotesque", color: "#ffffff", options: { size: 40 }, place: { dx: 10, dy: 60 } },
  ],
});

const failure = (call) => {
  try {
    call();
  } catch (error) {
    return error;
  }
  assert.fail("expected an error");
};
const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");

test("the canvas's size is read, and a render with no size is the canvas's long side", () => {
  const scene = Scene.fromJson(poster);
  assert.deepEqual([scene.width, scene.height], [120, 80]);
  const out = scene.render();
  assert.deepEqual([out.width, out.height, out.data.length], [120, 80, 120 * 80 * 4]);
  assert.deepEqual([scene.render({ size: 60 }).width, scene.render({ size: 60 }).height], [60, 40]);
});

test("parameters list each address in scene order with the parameter format's keys and the value it holds", () => {
  const listed = Scene.fromJson(poster).parameters();
  const addresses = listed.map((parameter) => parameter.address);
  assert.ok(addresses.indexOf("ground.color") < addresses.indexOf("title.color"));
  const color = listed.find((parameter) => parameter.address === "title.color");
  assert.deepEqual(
    [color.kind, color.value, color.unit, color.min, color.max, color.cyclic],
    ["color", "#ffffff", null, null, null, false],
  );
  assert.equal(typeof color.meaning, "string");
  const blend = listed.find((parameter) => parameter.address === "title.blend");
  assert.equal(blend.value, "normal");
  assert.ok(blend.choices.includes("multiply"));
  const opacity = listed.find((parameter) => parameter.address === "title.opacity");
  assert.deepEqual([opacity.value, opacity.min, opacity.max, opacity.unit], [1, 0, 1, "fraction"]);
  assert.equal("choices" in opacity, false);
});

test("a picture's parameter holds the path as the scene writes it", () => {
  const listed = Scene.fromJson(masked, { files: maskedFiles() }).parameters();
  const value = (address) => listed.find((parameter) => parameter.address === address).value;
  assert.equal(value("photo.picture"), "photo.png");
  assert.equal(value("photo.map.matte"), "maps/matte.png");
});

test("withValues of a colour changes the render and leaves the old scene's render as it was", () => {
  const scene = Scene.fromJson(poster);
  const before = scene.render().data;
  const red = scene.withValues({ "title.color": "#ff0000" });
  assert.equal(red.parameters().find((parameter) => parameter.address === "title.color").value, "#ff0000");
  assert.notEqual(differenceInLooks(red.render().data, before), null);
  assert.equal(differenceInLooks(scene.render().data, before), null);
  assert.equal(scene.parameters().find((parameter) => parameter.address === "title.color").value, "#ffffff");
});

test("withValues takes numbers, booleans, text and three numbers for a colour", () => {
  const set = Scene.fromJson(poster).withValues({
    "title.opacity": 0.5,
    "title.flip_x": true,
    "title.blend": "screen",
    "ground.look.light_leak.color": [1, 0.5, 0],
  });
  const value = (address) => set.parameters().find((parameter) => parameter.address === address).value;
  assert.deepEqual(
    [value("title.opacity"), value("title.flip_x"), value("title.blend"), value("ground.look.light_leak.color")],
    [0.5, true, "screen", [1, 0.5, 0]],
  );
});

test("a value of another type is a TypeError naming values and the address", () => {
  const scene = Scene.fromJson(poster);
  for (const wrong of [{}, null, undefined, [1, 0.5], [1, "0", 0], 10n]) {
    const error = failure(() => scene.withValues({ "title.color": wrong }));
    assert.equal(error.name, "TypeError", String(wrong));
    assert.match(error.message, /values\.title\.color/);
  }
  for (const values of [4, "title.color", null]) {
    const error = failure(() => scene.withValues(values));
    assert.equal(error.name, "TypeError");
    assert.match(error.message, /values/);
  }
});

test("an address the scene lacks is an InvalidError carrying the address", () => {
  const error = failure(() => Scene.fromJson(poster).withValues({ "nope.color": "#ff0000" }));
  assert.equal(error.name, "InvalidError");
  assert.ok(error instanceof Error);
  assert.match(error.address, /nope/);
  assert.match(error.message, /no such layer/);
});

test("a file the scene names that the files lack is a ReadError carrying its path as written", () => {
  const files = maskedFiles();
  delete files["maps/matte.png"];
  const error = failure(() => Scene.fromJson(masked, { files }));
  assert.equal(error.name, "ReadError");
  assert.equal(error.path, "maps/matte.png");
  assert.equal(failure(() => Scene.fromJson(masked)).path, "photo.png");
});

test("a layer of a kind made by a plugin is a FormatError naming the kind: the package has none", () => {
  const made = JSON.stringify({ version: 1, canvas: { width: 4, height: 4 }, layers: [{ make: { kind: "noise", options: {} } }] });
  const error = failure(() => Scene.fromJson(made));
  assert.equal(error.name, "FormatError");
  assert.equal(error.pointer, "/layers/0/make/kind");
  assert.match(error.message, /noise/);
});

test("a document that is no scene is a FormatError carrying the JSON Pointer", () => {
  const error = failure(() => Scene.fromJson(JSON.stringify({ version: 1, canvas: { width: 4, height: 4 }, layers: [{ fill: "#ffffff", opacity: "x" }] })));
  assert.equal(error.name, "FormatError");
  assert.equal(error.pointer, "/layers/0/opacity");
});

test("a size that is no whole number is a TypeError, one outside the range an InvalidError, both naming size", () => {
  const scene = Scene.fromJson(poster);
  for (const size of [1.5, "9", null, NaN, Infinity]) {
    const error = failure(() => scene.render({ size }));
    assert.equal(error.name, "TypeError", String(size));
    assert.match(error.message, /size/);
  }
  for (const size of [0, -4, 65001]) {
    const error = failure(() => scene.render({ size }));
    assert.equal(error.name, "InvalidError", String(size));
    assert.equal(error.address, "size");
  }
  assert.match(failure(() => scene.render(3)).message, /options/);
});

test("a text that is no string is a TypeError naming text", () => {
  for (const text of [42, undefined, {}, { version: 1 }, null]) {
    const error = failure(() => Scene.fromJson(text));
    assert.equal(error.name, "TypeError", String(text));
    assert.match(error.message, /text/);
  }
});

test("files are Uint8Arrays by path; anything else is a TypeError naming it", () => {
  const error = failure(() => Scene.fromJson(masked, { files: { "a.png": "x" } }));
  assert.equal(error.name, "TypeError");
  assert.match(error.message, /files\.a\.png/);
  assert.match(failure(() => Scene.fromJson(masked, { files: 3 })).message, /files/);
  assert.match(failure(() => Scene.fromJson(masked, "files")).message, /options/);
});

test("cacheBytes is a whole number of bytes the cache keeps at most; anything else is a TypeError naming it", () => {
  for (const cacheBytes of [-1, 1.5, "64", null, NaN, Infinity, 2 ** 32]) {
    const error = failure(() => Scene.fromJson(poster, { cacheBytes }));
    assert.equal(error.name, "TypeError", String(cacheBytes));
    assert.match(error.message, /cacheBytes/);
  }
  // a budget changes no picture: a cache that keeps nothing draws every layer again, after a first render as before it
  const cold = Scene.fromJson(masked, { files: maskedFiles() }).withValues({ "photo.opacity": 0.5 }).render({ size: 200 }).data;
  for (const cacheBytes of [0, 2 ** 32 - 1]) {
    const scene = Scene.fromJson(masked, { files: maskedFiles(), cacheBytes });
    scene.render({ size: 200 });
    assert.equal(differenceInLooks(scene.withValues({ "photo.opacity": 0.5 }).render({ size: 200 }).data, cold), null, String(cacheBytes));
  }
});

test("a scene written by toJson and read back with the same files renders the same picture", () => {
  const files = maskedFiles();
  const saved = Scene.fromJson(masked, { files }).toJson();
  // both renders are the first of their scene: neither is answered from a cache
  const first = Scene.fromJson(masked, { files }).render({ size: 200 }).data;
  const again = Scene.fromJson(saved, { files }).render({ size: 200 }).data;
  assert.equal(sha256(again), sha256(first));
  assert.match(JSON.parse(saved).layers[1].picture.sha256, /^[0-9a-f]{64}$/);
});

test("a render is straight alpha, each level rounded to 8 bits as a file keeps it", () => {
  const half = JSON.stringify({ version: 1, canvas: { width: 4, height: 2 }, layers: [{ fill: "#ff0000", opacity: 0.5 }] });
  const { data } = Scene.fromJson(half).render();
  // by hand: red stays 1 (straight, not 0.5 premultiplied); alpha 0.5 * 255 + 0.5 = 128 cut
  assert.deepEqual([...data.subarray(0, 4)], [255, 0, 0, 128]);
  assert.deepEqual([...data.subarray(data.length - 4)], [255, 0, 0, 128]);
});

test("a render is a plain object whose data is a JavaScript array to keep", () => {
  const out = Scene.fromJson(poster).render({ size: 12 });
  assert.equal(Object.getPrototypeOf(out), Object.prototype);
  assert.deepEqual(Object.keys(out).sort(), ["data", "height", "width"]);
  assert.ok(out.data instanceof Uint8ClampedArray);
});

// 100 by 100: paper, a red card (10..50), a group moved 50 right holding `a` (0..10 by 80..90)
const editable = JSON.stringify({
  version: 1,
  canvas: { width: 100, height: 100 },
  layers: [
    { name: "paper", fill: "#ffffff" },
    { name: "card", fill: "#ff0000", fit: { box: [10, 10, 40, 40], how: "stretch" } },
    {
      name: "g",
      scene: { version: 1, canvas: { width: 100, height: 100 }, layers: [{ name: "a", fill: "#ff00ff", fit: { box: [0, 80, 10, 10], how: "stretch" } }] },
      place: { dx: 50 },
    },
  ],
});

test("layerAt is the address of the topmost layer that shows at a pixel of the render at size, or null", () => {
  const scene = Scene.fromJson(editable);
  assert.equal(scene.layerAt(20, 20, { size: 100 }), "card");
  assert.equal(scene.layerAt(5, 5, { size: 100 }), "paper");
  assert.equal(scene.layerAt(52, 82, { size: 100 }), "g.scene.a");
  assert.equal(scene.layerAt(10, 10, { size: 50 }), "card", "the pixel of the picture at size 50");
  assert.equal(scene.layerAt(20, 20), "card", "size defaults to the canvas's long side");
  assert.equal(Scene.fromJson(JSON.stringify({ version: 1, canvas: { width: 4, height: 4 }, layers: [] })).layerAt(1, 1), null);
});

test("layerAt refuses a pixel outside the picture by an InvalidError, and a point that is no whole number by a TypeError", () => {
  const scene = Scene.fromJson(editable);
  const outside = failure(() => scene.layerAt(100, 0, { size: 100 }));
  assert.equal(outside.name, "InvalidError");
  assert.equal(outside.address, "x");
  for (const [x, y, named] of [[1.5, 0, /x/], [0, "3", /y/], [0, null, /y/]]) {
    const error = failure(() => scene.layerAt(x, y));
    assert.equal(error.name, "TypeError", String([x, y]));
    assert.match(error.message, named);
  }
});

test("withLayers takes the layers as the document writes them: a reorder keeps the addresses, a removal drops its own", () => {
  const scene = Scene.fromJson(editable).withValues({ "card.dx": 3 });
  const layers = JSON.parse(scene.toJson()).layers;
  const addresses = (s) => s.parameters().map((parameter) => parameter.address).sort();
  const reordered = scene.withLayers([layers[1], layers[0], layers[2]]);
  assert.deepEqual(addresses(reordered), addresses(scene));
  assert.equal(reordered.parameters().find((parameter) => parameter.address === "card.dx").value, 3);
  assert.equal(reordered.layerAt(20, 20, { size: 100 }), "paper", "the paper is over the card now");
  const without = scene.withLayers(layers.filter((layer) => layer.name !== "card"));
  assert.ok(addresses(without).every((address) => !address.startsWith("card.")));
  assert.equal(without.layerAt(20, 20, { size: 100 }), "paper");
  // the same picture as a scene read afresh from the edited document
  const afresh = Scene.fromJson(JSON.stringify({ ...JSON.parse(editable), layers: [layers[1], layers[0], layers[2]] }));
  assert.equal(differenceInLooks(reordered.render().data, afresh.render().data), null);
  assert.equal(scene.parameters().length, Scene.fromJson(editable).parameters().length, "the scene itself is left as it is");
});

test("withLayers reads files as fromJson does, and names a bad layer by its JSON Pointer", () => {
  const files = maskedFiles();
  const scene = Scene.fromJson(masked, { files });
  const layers = JSON.parse(scene.toJson()).layers;
  assert.equal(failure(() => scene.withLayers(layers)).name, "ReadError", "the files the layers name are handed in again");
  assert.equal(differenceInLooks(scene.withLayers(layers, { files }).render({ size: 200 }).data, scene.render({ size: 200 }).data), null);
  const bad = failure(() => scene.withLayers([{ fill: "#ffffff", opacity: "x" }], { files }));
  assert.equal(bad.name, "FormatError");
  assert.equal(bad.pointer, "/layers/0/opacity");
  for (const layersGiven of [{}, "[]", null]) {
    const error = failure(() => scene.withLayers(layersGiven));
    assert.equal(error.name, "TypeError");
    assert.match(error.message, /layers/);
  }
});
