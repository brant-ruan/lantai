import { useT } from "../i18n";

/** Small label naming the coding agent a session came from (Claude Code, Codex CLI, ...). */
export function AgentBadge({ agent, name }: { agent: string; name: string }) {
  const t = useT();
  return (
    <span className={`agent-badge agent-${agent}`} title={t.agentBadge.source(name)}>
      {name}
    </span>
  );
}
