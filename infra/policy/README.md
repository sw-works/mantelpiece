MemoryLane tool policies for the AgentCore Gateway (constitution III, contracts/tools.md).
AgentCore Policy takes exactly one Cedar statement per Policy resource, so each file here
is one statement; the stack creates a Policy per file, in name order. Each Gateway tool is
an action named "<target>___<tool>" and the tool arguments arrive as `context.input`.
The API Lambda enforces the same rules on every call regardless (F-504).
