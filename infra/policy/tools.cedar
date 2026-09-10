// MemoryLane tool policy for the AgentCore Gateway (constitution III, contracts/tools.md).
//
// Model: AgentCore Policy evaluates Cedar per tool call. Each Gateway tool is an action
// named "<target>___<tool>" and the tool arguments arrive as `context.input`. Written from
// the AgentCore Policy documentation and not yet validated against a deployed policy
// engine; the API Lambda enforces the same rules on every call regardless (F-504).
//
// Rule 1: no irreversible tool runs without a confirmation token. The token is minted only
// by a TV or phone session of the owner or a co-editor and redeemed once by the API, which
// also checks it names this exact action and, for RemoveMedia, that the confirming subject
// holds the co-editor role or owns the trip. Cedar sees only that a token was supplied.
forbid(
  principal,
  action in [
    AgentCore::Action::"api___apply_dedup",
    AgentCore::Action::"api___apply_cleanup",
    AgentCore::Action::"api___delete_media",
    AgentCore::Action::"api___share_album",
    AgentCore::Action::"api___remove_contributor"
  ],
  resource
) unless {
  context has input &&
  context.input has confirmation &&
  context.input.confirmation != ""
};

// Rule 2: removing a contributor is the owner's call alone; agents may only propose it.
forbid(
  principal,
  action == AgentCore::Action::"api___remove_contributor",
  resource
) unless {
  context has input &&
  context.input has requested_by &&
  context.input.requested_by == "owner"
};

// Everything else on the Gateway (reads, reversible writes, proposals) is allowed for any
// authenticated principal; the API scopes every call to the caller's household.
permit(principal, action, resource);
