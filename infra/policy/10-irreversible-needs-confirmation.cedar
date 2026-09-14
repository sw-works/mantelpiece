// No irreversible tool runs without a confirmation token. The token is minted only by a
// TV or phone session of the owner or a co-editor and redeemed once by the API, which also
// checks it names this exact action and, for RemoveMedia, that the confirming subject holds
// the co-editor role or owns the trip. Cedar sees only that a token was supplied.
forbid(
  principal is AgentCore::OAuthUser,
  action in [
    AgentCore::Action::"api___apply_dedup",
    AgentCore::Action::"api___apply_cleanup",
    AgentCore::Action::"api___delete_media",
    AgentCore::Action::"api___share_album",
    AgentCore::Action::"api___remove_contributor"
  ],
  resource == AgentCore::Gateway::"${GATEWAY_ARN}"
) unless {
  context has input &&
  context.input has confirmation &&
  context.input.confirmation != ""
};
