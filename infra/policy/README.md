MemoryLane tool policies for the AgentCore Gateway (constitution III, contracts/tools.md).
AgentCore Policy takes exactly one Cedar statement per Policy resource, so each file here
is one statement; the stack creates a Policy per file, in name order. Each Gateway tool is
an action named "<target>___<tool>" and the tool arguments arrive as `context.input`.
The API Lambda enforces the same rules on every call regardless (F-504).

Rules the service enforces at deploy time (decision 31):
- No wildcard resource. A rule that names actions must name the Gateway; write
  `resource == AgentCore::Gateway::"${GATEWAY_ARN}"` and the stack fills the ARN.
  An untargeted rule uses `resource is AgentCore::Gateway`.
- No permit-everything rule ("Overly Permissive"). The permit for OAuth callers is
  generated from infra/lib/tools.ts with an explicit action list, not kept here.
- Actions are checked against the Gateway's registered tools, so the stack creates every
  Policy after the `api` target; a tool that is not in tools.ts is "unrecognized".
- A forbid must name a principal type that some permit covers (`principal is
  AgentCore::OAuthUser`); an unscoped `principal` is "Overly Restrictive" because it also
  covers `AgentCore::IamEntity`, which nothing permits. Forbids are created after the permit.
