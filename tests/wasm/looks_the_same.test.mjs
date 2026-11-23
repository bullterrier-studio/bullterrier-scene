// The comparison tests/wasm judges renders with: pairs it must accept and pairs it must refuse.
import assert from "node:assert/strict";
import { test } from "node:test";

import { differenceInLooks } from "./looks_the_same.mjs";

// 1,000 samples, so one of them may be two levels off and none three.
const base = () => new Uint8Array(1000).fill(100);
const moved = (...changes) => {
  const other = base();
  for (const [at, levels] of changes) other[at] += levels;
  return other;
};

test("samples one level off, and one in a thousand two levels off, look the same", () => {
  assert.equal(differenceInLooks(moved([1, 1], [2, -1], [500, 1], [999, 1]), base()), null);
  assert.equal(differenceInLooks(moved([3, 1], [700, -2]), base()), null);
});

test("two in a thousand samples two levels off (0.2 %) do not look the same", () => {
  assert.match(differenceInLooks(moved([4, 2], [8, 2]), base()), /2 of 1000 samples .*first at 4/);
});

test("one sample three levels off does not look the same", () => {
  assert.match(differenceInLooks(moved([7, 3]), base()), /largest is 3 levels, first at 7/);
});

test("pictures of other lengths do not look the same", () => {
  assert.match(differenceInLooks(base().subarray(4), base()), /lengths differ/);
});
