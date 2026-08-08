export interface DraftCloseActions {
  journalDirty: boolean;
  templateDirty: boolean;
  preventClose: () => void;
  flushJournal: () => Promise<boolean>;
  flushTemplate: () => Promise<boolean>;
  destroyWindow: () => Promise<void>;
}

export type DraftCloseResult = "allow" | "closed" | "blocked";

/**
 * Prevents a dirty window from closing until both draft stores have completed.
 * A rejected or unsuccessful write leaves the window open so the user can retry.
 */
export async function persistDraftsBeforeClose(
  actions: DraftCloseActions,
): Promise<DraftCloseResult> {
  if (!actions.journalDirty && !actions.templateDirty) return "allow";

  actions.preventClose();
  const [journal, template] = await Promise.allSettled([
    actions.flushJournal(),
    actions.flushTemplate(),
  ]);
  const journalOk = journal.status === "fulfilled" && journal.value;
  const templateOk = template.status === "fulfilled" && template.value;
  if (!journalOk || !templateOk) return "blocked";

  await actions.destroyWindow();
  return "closed";
}

/**
 * Conservative recovery rule: uncertain timestamps and orphaned drafts are
 * retained for user review instead of being deleted automatically.
 */
export function isRecoverableDraft(
  draftUpdatedAt: string,
  entryUpdatedAt?: string | null,
): boolean {
  if (!entryUpdatedAt) return true;
  const draftTime = Date.parse(draftUpdatedAt);
  const entryTime = Date.parse(entryUpdatedAt);
  if (!Number.isFinite(draftTime) || !Number.isFinite(entryTime)) return true;
  return draftTime > entryTime;
}
