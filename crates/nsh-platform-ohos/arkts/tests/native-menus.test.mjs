import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { stripTypeScriptTypes } from "node:module";
import test from "node:test";
const source = await readFile(new URL("../src/main/ets/NativeMenus.ets", import.meta.url), "utf8");
const { NativeMenuCleanup } = await import(
  `data:text/javascript;base64,${Buffer.from(stripTypeScriptTypes(source)).toString("base64")}`
);
test("the disposal barrier waits for platform close before disposing its content", async () => {
  const cleanup = new NativeMenuCleanup();
  let finish;
  const close = new Promise((resolve) => {
    finish = resolve;
  });
  let disposed = false,
    settled = false;
  cleanup.retire(
    {},
    Promise.resolve(),
    () => close,
    () => {
      disposed = true;
    },
  );
  const barrier = cleanup.settled().then(() => {
    settled = true;
  });
  await Promise.resolve();
  assert.equal(disposed, false);
  assert.equal(settled, false);
  finish();
  await barrier;
  assert.equal(disposed, true);
});
test("one close failure retains its owner and does not block other menu cleanup", async () => {
  const cleanup = new NativeMenuCleanup();
  let disposed = 0;
  cleanup.retire(
    {},
    Promise.resolve(),
    async () => {
      throw new Error("close failed");
    },
    () => assert.fail("open content disposed"),
  );
  cleanup.retire(
    {},
    Promise.reject(new Error("open failed")),
    async () => {},
    () => {
      disposed++;
    },
  );
  await cleanup.settled();
  assert.equal(disposed, 1);
});
