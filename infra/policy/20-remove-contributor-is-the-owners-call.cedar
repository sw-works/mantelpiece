// Removing a contributor is the owner's call alone; agents may only propose it.
forbid(
  principal,
  action == AgentCore::Action::"api___remove_contributor",
  resource
) unless {
  context has input &&
  context.input has requested_by &&
  context.input.requested_by == "owner"
};
