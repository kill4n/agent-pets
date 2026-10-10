import type { AgentView } from "../shared/types";

const LABEL: Record<string, string> = {
  working: "trabajando", idle: "idle", needs_input: "necesita tu respuesta",
  delegating: "delegando a subagentes", waiting_peer: "esperando a otro agente", gone: "terminó",
};

export function shortCwd(cwd: string | null): string {
  if (!cwd) return "";
  const parts = cwd.split("/").filter(Boolean);
  return parts.length <= 2 ? cwd : "…/" + parts.slice(-2).join("/");
}

export function fmtSince(s: number): string {
  if (s < 60) return `${s}s`;
  if (s < 3600) return `${Math.floor(s / 60)}m`;
  return `${Math.floor(s / 3600)}h ${Math.floor((s % 3600) / 60)}m`;
}

export function render(el: HTMLElement, v: AgentView, sinceOffset: number) {
  const detail = v.visible === "needs_input" && v.waiting_for ? ` (${v.waiting_for})`
               : v.visible === "waiting_peer" && v.peer ? ` → ${v.peer}` : "";
  el.innerHTML = `<b>${v.name}</b><br>${shortCwd(v.cwd)}<br>${LABEL[v.visible] ?? v.visible}${detail} · ${fmtSince(v.since_secs + sinceOffset)}`;
}
