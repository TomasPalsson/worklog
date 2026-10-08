// Pure visibility predicates for the per-ticket-group actions on the
// day-review page. Keeping them out of the React tree means we can
// unit-test the decision logic without a render harness.
//
// Two actions live on the day page:
//   - "Merge all" — shown on the ticket-group summary when there is
//     more than one block sharing the same ticket. Hidden on the
//     unassigned bucket (no shared ticket to merge under) and on
//     solo groups (nothing to merge).
//   - Sparkles "Describe with Claude" — shown on every unassigned block
//     (each is its own line; Claude may also pick its ticket) and on a
//     block that is the single surviving block in its assigned ticket
//     group. Hidden on members of a multi-block assigned group (merge
//     first so the description covers the whole logged time).

/** Minimal shape needed to decide group-level actions. */
export interface GroupShape {
  unassigned: boolean;
  blocks: ReadonlyArray<unknown>;
}

/** Minimal shape needed to decide per-block actions. */
export interface BlockShape {
  jira_issue: string | null;
  is_personal: boolean;
  rough_note?: string | null;
}

export function canMergeGroup(group: GroupShape): boolean {
  if (group.unassigned) return false;
  return group.blocks.length >= 2;
}

export function shouldShowSparkles(
  block: BlockShape,
  isSoleInGroup: boolean,
): boolean {
  if (block.rough_note) return true;
  if (block.is_personal) return false;
  if (!block.jira_issue) return true;
  return isSoleInGroup;
}
