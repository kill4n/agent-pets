import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { enable, disable, isEnabled } from "@tauri-apps/plugin-autostart";
import type { AgentView } from "../shared/types";

interface Status {
  hooks_installed: boolean; poll_error: string | null; server_error: string | null; pack_warning: string | null;
  hook_url: string; token: string; pack: string; size: "small" | "normal" | "large"; wander: boolean; wander_after_secs: number; show_name: boolean;
}

const app = document.getElementById("app")!;
let status: Status; let agents: AgentView[] = []; let packs: { dir: string; name: string }[] = []; let autostart = false;

// All dynamic values go through textContent: agent strings are untrusted.
function el<K extends keyof HTMLElementTagNameMap>(
  tag: K, text?: string, attrs: Record<string, string> = {}, ...children: (Node | string)[]
): HTMLElementTagNameMap[K] {
  const n = document.createElement(tag);
  if (text !== undefined) n.textContent = text;
  for (const [k, v] of Object.entries(attrs)) n.setAttribute(k, v);
  n.append(...children);
  return n;
}

function render() {
  const hooksBtn = el("button", status.hooks_installed ? "Disconnect" : "Connect to Claude Code", { id: "hooks" });
  hooksBtn.onclick = async () => {
    try { await invoke(status.hooks_installed ? "uninstall_hooks" : "install_hooks"); }
    catch (e) { alert(`Error: ${e}`); }
    await refresh();
  };

  const packSel = el("select", undefined, { id: "pack" });
  for (const p of packs) {
    const o = el("option", p.name, { value: p.dir });
    o.selected = p.dir === status.pack;
    packSel.append(o);
  }
  packSel.onchange = async () => {
    try { await invoke("set_pack", { name: packSel.value }); } catch (err) { alert(`Invalid pack: ${err}`); }
    await refresh();
  };

  const auto = el("input", undefined, { type: "checkbox", id: "auto" });
  auto.checked = autostart;
  auto.onchange = async () => {
    try { auto.checked ? await enable() : await disable(); } catch (err) { alert(`Error: ${err}`); }
    autostart = await isEnabled().catch(() => false);
    auto.checked = autostart;
  };

  const showName = el("input", undefined, { type: "checkbox", id: "show-name" });
  showName.checked = status.show_name;
  showName.onchange = async () => {
    try { await invoke("set_show_name", { enabled: showName.checked }); } catch (err) { alert(`Error: ${err}`); }
    await refresh();
  };
  const wander = el("input", undefined, { type: "checkbox", id: "wander" });
  wander.checked = status.wander;
  const wanderAfter = el("input", undefined, { type: "number", id: "wander-after", min: "5", max: "3600", step: "5", style: "width:5em" });
  wanderAfter.value = String(status.wander_after_secs);
  const saveWander = async () => {
    const secs = Math.round(Number(wanderAfter.value));
    try { await invoke("set_wander", { enabled: wander.checked, afterSecs: secs }); } catch (err) { alert(`Error: ${err}`); }
    await refresh();
  };
  wander.onchange = saveWander;
  wanderAfter.onchange = saveWander;

  const list = el("ul");
  if (agents.length === 0) list.append(el("li", "none"));
  for (const a of agents) {
    const li = el("li", undefined, {}, el("b", a.name), ` · ${a.visible}`);
    if (a.waiting_for) li.append(` (${a.waiting_for})`);
    if (a.peer) li.append(` → ${a.peer}`);
    li.append(el("br"), el("code", a.cwd ?? ""));
    list.append(li);
  }

  const sizeSel = el("select", undefined, { id: "size" });
  for (const [v, label] of [["small", "Small"], ["normal", "Normal"], ["large", "Large"]] as const) {
    const o = el("option", label, { value: v });
    o.selected = v === status.size;
    sizeSel.append(o);
  }
  sizeSel.onchange = async () => {
    try { await invoke("set_size", { size: sizeSel.value }); } catch (err) { alert(`Invalid size: ${err}`); }
    await refresh();
  };

  app.replaceChildren(
    el("h2", "Claude Code"),
    el("p", "Hooks: ", {},
      el("span", status.hooks_installed ? "installed" : "not installed", { class: status.hooks_installed ? "ok" : "bad" }),
      " ", hooksBtn),
    el("p", "Poll: ", {},
      status.poll_error ? el("span", `⚠ ${status.poll_error}`, { class: "warn" }) : el("span", "ok", { class: "ok" })),
    el("p", "Server: ", {}, el("code", status.hook_url), " ",
      status.server_error ? el("span", `⚠ ${status.server_error}`, { class: "warn" }) : el("span", "ok", { class: "ok" }),
      el("br"), "Token: ", el("code", status.token)),
    el("h2", "Sprites"),
    el("label", "Pack ", {}, packSel),
    el("label", "Size ", {}, sizeSel),
    el("label", undefined, {}, showName, " Show session name"),
    el("label", undefined, {}, wander, " Wander when idle"),
    el("label", "Start wandering after ", {}, wanderAfter, " seconds idle"),
    ...(status.pack_warning ? [el("p", `⚠ ${status.pack_warning}`, { class: "warn" })] : []),
    el("h2", "System"),
    el("label", undefined, {}, auto, " Launch at login"),
    el("h2", `Agents (${agents.length})`),
    list,
  );
}

async function refresh() {
  [status, agents, packs, autostart] = await Promise.all([
    invoke<Status>("get_status"), invoke<AgentView[]>("list_agents"), invoke<{ dir: string; name: string }[]>("list_pack_names"), isEnabled().catch(() => false),
  ]);
  render();
}

listen<AgentView[]>("agents", (e) => { agents = e.payload; if (status) render(); });
refresh();
setInterval(() => invoke<Status>("get_status").then((s) => { if (JSON.stringify(s) !== JSON.stringify(status)) { status = s; render(); } }), 3000);
