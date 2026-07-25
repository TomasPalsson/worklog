// Pure visibility predicates for the per-ticket-group actions on the
// day-review page. Keeping them out of the React tree means we can
// unit-test the decision logic without a render harness.
//
// Two actions live on the day page:
//   - "Merge all" — shown on the ticket-group summary when there is
//     more than one block sharing the same ticket. Hidden on the
//     unassigned bucket (no shared ticket to merge under) and on
//     solo groups (nothing to merge).
//   - Sparkles "Describe with Claude" — shown on a block whenever it
//     is the single surviving block in its assigned ticket group, i.e.
//     the merged primary OR a block that already started alone. Hidden
//     on unassigned blocks (the prompt needs ticket context — assign
//     first) and on members of a multi-block group (merge first so the
//     description covers the whole logged time).

/** Minimal shape needed to decide group-level actions. */
export interface GroupShape {
  unassigned: boolean;
  blocks: ReadonlyArray<unknown>;
}

/** Minimal shape needed to decide per-block actions. */
export interface BlockShape {
  jira_issue: string | null;
  is_personal: boolean;
}

export function canMergeGroup(group: GroupShape): boolean {
  if (group.unassigned) return false;
  return group.blocks.length >= 2;
}

export function shouldShowSparkles(
  block: BlockShape,
  isSoleInGroup: boolean,
): boolean {
  if (block.is_personal) return false;
  if (!block.jira_issue) return false;
  return isSoleInGroup;
}
