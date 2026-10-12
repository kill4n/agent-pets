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

export type Decision = "allow" | "deny" | "terminal";

export function render(el: HTMLElement, v: AgentView, sinceOffset: number, onDecide?: (d: Decision) => void) {
  const detail = v.visible === "needs_input" && v.waiting_for ? ` (${v.waiting_for})`
               : v.visible === "waiting_peer" && v.peer ? ` → ${v.peer}` : "";
  const b = document.createElement("b");
  b.textContent = v.name;
  el.replaceChildren(
    b,
    document.createElement("br"),
    document.createTextNode(shortCwd(v.cwd)),
    document.createElement("br"),
    document.createTextNode(`${LABEL[v.visible] ?? v.visible}${detail} · ${fmtSince(v.since_secs + sinceOffset)}`),
  );
  if (v.permission && onDecide) {
    const [tool, summary] = v.permission;
    const ask = document.createElement("div");
    ask.className = "ask";
    const what = document.createElement("code");
    what.textContent = summary ? `${tool}: ${summary}` : tool;
    const actions = document.createElement("div");
    actions.className = "actions";
    const truncated = summary.endsWith("[… truncado: decide en el terminal]");
    const choices = truncated
      ? ([["Denegar", "deny", "bad"], ["En el terminal", "terminal", ""]] as const)
      : ([["Permitir", "allow", "ok"], ["Denegar", "deny", "bad"], ["En el terminal", "terminal", ""]] as const);
    for (const [label, d, cls] of choices) {
      const b = document.createElement("button");
      b.textContent = label; b.className = cls; b.type = "button";
      b.addEventListener("mousedown", (e) => e.stopPropagation());
      b.addEventListener("mouseup", (e) => e.stopPropagation());
      b.addEventListener("click", (e) => { e.stopPropagation(); onDecide(d); });
      actions.append(b);
    }
    ask.append(what, actions);
    el.append(ask);
  }
}
