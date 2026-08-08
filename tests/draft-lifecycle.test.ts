import assert from "node:assert/strict";
import test from "node:test";

import {
  isRecoverableDraft,
  persistDraftsBeforeClose,
} from "../frontend/src/utils/draftLifecycle.ts";

test("dirty window waits for both draft saves before closing", async () => {
  const events: string[] = [];
  let releaseJournal!: (value: boolean) => void;
  const journalWrite = new Promise<boolean>((resolve) => {
    releaseJournal = resolve;
  });

  const closing = persistDraftsBeforeClose({
    journalDirty: true,
    templateDirty: false,
    preventClose: () => events.push("prevent"),
    flushJournal: () => journalWrite,
    flushTemplate: async () => {
      events.push("template-saved");
      return true;
    },
    destroyWindow: async () => {
      events.push("destroy");
    },
  });

  await Promise.resolve();
  assert.deepEqual(events, ["prevent", "template-saved"]);
  releaseJournal(true);
  assert.equal(await closing, "closed");
  assert.deepEqual(events, ["prevent", "template-saved", "destroy"]);
});

test("failed draft save keeps the closing window open", async () => {
  let destroyed = false;
  const result = await persistDraftsBeforeClose({
    journalDirty: true,
    templateDirty: true,
    preventClose: () => {},
    flushJournal: async () => false,
    flushTemplate: async () => true,
    destroyWindow: async () => {
      destroyed = true;
    },
  });

  assert.equal(result, "blocked");
  assert.equal(destroyed, false);
});

test("forced-termination drafts are retained conservatively", () => {
  assert.equal(isRecoverableDraft("2026-08-08T10:01:00Z", null), true);
  assert.equal(
    isRecoverableDraft("2026-08-08T10:01:00Z", "2026-08-08T10:00:00Z"),
    true,
  );
  assert.equal(
    isRecoverableDraft("2026-08-08T09:59:00Z", "2026-08-08T10:00:00Z"),
    false,
  );
  assert.equal(isRecoverableDraft("invalid", "2026-08-08T10:00:00Z"), true);
});
