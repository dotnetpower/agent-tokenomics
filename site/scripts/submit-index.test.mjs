import test from "node:test";
import assert from "node:assert/strict";
import { indexNowPayloads, submitIndex } from "./submit-index.mjs";
import { root, indexNowKey } from "./site-contract.mjs";

test("IndexNow batches are host/base scoped and contain an explicit subpath keyLocation", () => {
  const urls = Array.from({ length: 10001 }, (_, i) => `${root}page-${i}/`);
  const payloads = indexNowPayloads(urls);
  assert.deepEqual(payloads.map((payload) => payload.urlList.length), [10000, 1]);
  assert.equal(payloads[0].keyLocation, `${root}${indexNowKey}.txt`);
  assert.throws(() => indexNowPayloads(["https://example.net/private/"]));
  assert.throws(() => indexNowPayloads([]));
});

test("submission verifies the live key and uses POST IndexNow + PUT GSC", async () => {
  const calls = [];
  await submitIndex({
    urls: [root], token: "synthetic-test-token",
    fetcher: async (url, options) => {
      calls.push({ url, options });
      return new Response(calls.length === 1 ? indexNowKey : "", { status: 200 });
    },
  });
  assert.equal(calls[1].options.method, "POST");
  assert.deepEqual(JSON.parse(calls[1].options.body).urlList, [root]);
  assert.equal(calls[2].options.method, "PUT");
  assert.match(calls[2].url, /\/sitemaps\//);
});

test("submission fails explicitly when the deployed key is wrong", async () => {
  await assert.rejects(submitIndex({
    urls: [root], token: "synthetic-test-token", fetcher: async () => new Response("wrong key"),
  }), /deployed IndexNow key/);
});

test("missing GSC credentials prevent any external requests", async () => {
  await assert.rejects(submitIndex({
    urls: [root], fetcher: async () => assert.fail("must not send a request without credentials"),
  }), /GSC_ACCESS_TOKEN is required/);
});

test("IndexNow rejection prevents a misleading GSC success", async () => {
  let requests = 0;
  await assert.rejects(submitIndex({
    urls: [root], token: "synthetic-test-token",
    fetcher: async () => ++requests === 1 ? new Response(indexNowKey) : new Response("", { status: 429 }),
  }), /IndexNow submission failed: HTTP 429/);
  assert.equal(requests, 2);
});
