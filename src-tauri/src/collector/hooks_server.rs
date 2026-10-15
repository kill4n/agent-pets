use super::event::{parse_hook, AgentEvent, EventKind};
use axum::{body::Bytes, extract::State, http::{HeaderMap, StatusCode}, response::IntoResponse, routing::{get, post}, Router};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};

/// What the user decided for a parked permission prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision { Allow, Deny, Terminal }

impl Decision {
    pub fn parse(s: &str) -> Option<Decision> {
        match s { "allow" => Some(Decision::Allow), "deny" => Some(Decision::Deny), "terminal" => Some(Decision::Terminal), _ => None }
    }
}

/// PermissionRequest hooks held open until the user answers from the bubble (or the wait expires),
/// keyed by agent key (`host:session_id`). One at a time per session: a newer request replaces the
/// older one, which is released without a decision.
#[derive(Default)]
pub struct Parked { map: Mutex<HashMap<String, oneshot::Sender<Decision>>> }

impl Parked {
    pub fn park(&self, key: String) -> oneshot::Receiver<Decision> {
        let (tx, rx) = oneshot::channel();
        if let Some(old) = self.map.lock().unwrap().insert(key, tx) { let _ = old.send(Decision::Terminal); }
        rx
    }
    /// Resolve a parked request; false when nothing was waiting for that key.
    pub fn answer(&self, key: &str, d: Decision) -> bool {
        match self.map.lock().unwrap().remove(key) { Some(tx) => tx.send(d).is_ok(), None => false }
    }
    pub fn is_parked(&self, key: &str) -> bool { self.map.lock().unwrap().contains_key(key) }
    fn forget(&self, key: &str) { self.map.lock().unwrap().remove(key); }
}

/// JSON body answering a PermissionRequest hook. No decision → `{}` and Claude Code prompts as usual.
pub fn permission_response(d: Option<Decision>) -> serde_json::Value {
    match d {
        Some(Decision::Allow) => serde_json::json!({"hookSpecificOutput": {"hookEventName": "PermissionRequest", "decision": {"behavior": "allow"}}}),
        Some(Decision::Deny) => serde_json::json!({"hookSpecificOutput": {"hookEventName": "PermissionRequest", "decision": {"behavior": "deny", "message": "Denied from CodePets"}}}),
        Some(Decision::Terminal) | None => serde_json::json!({}),
    }
}

#[derive(Clone)]
struct AppState {
    host: String,
    token: Arc<String>,
    tx: mpsc::Sender<AgentEvent>,
    parked: Arc<Parked>,
    /// how long to hold a permission prompt for the bubble before letting the terminal ask
    wait: Arc<Mutex<Duration>>,
}

pub fn router(host: String, token: String, tx: mpsc::Sender<AgentEvent>, parked: Arc<Parked>, wait: Arc<Mutex<Duration>>) -> Router {
    Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/hook", post(hook))
        .with_state(AppState { host, token: Arc::new(token), tx, parked, wait })
}

async fn hook(State(st): State<AppState>, headers: HeaderMap, body: Bytes) -> axum::response::Response {
    let expected = format!("Bearer {}", st.token);
    let ok = headers.get("authorization").and_then(|v| v.to_str().ok()) == Some(expected.as_str());
    if !ok { return StatusCode::UNAUTHORIZED.into_response(); }
    let v = match serde_json::from_slice::<serde_json::Value>(&body) {
        Ok(v) => v,
        Err(e) => { log::warn!("hook body not json: {e}"); return StatusCode::OK.into_response(); }
    };
    let ev = match parse_hook(&st.host, &v) {
        Ok(ev) => ev,
        Err(e) => { log::warn!("hook dropped: {e}"); return StatusCode::OK.into_response(); }
    };
    let EventKind::Hook(h) = &ev.kind else { return StatusCode::OK.into_response() };
    log::debug!("hook {} session={} agent={:?} type={:?} tool={:?}", h.event, ev.session_id, h.agent_id, h.agent_type, h.tool_name);
    let is_permission = h.event == "PermissionRequest" && h.agent_id.is_none();
    let key = format!("{}:{}", ev.host, ev.session_id);
    let _ = st.tx.try_send(ev);
    if !is_permission { return StatusCode::OK.into_response(); }

    // Hold the request so the user can answer from the sprite's bubble. Any other hook from the
    // session (the user answered in the terminal, Esc, ...) does not reach here, so we also give up
    // after the configured wait and let Claude Code show its own prompt.
    let wait = *st.wait.lock().unwrap();
    let rx = st.parked.park(key.clone());
    let decision = match tokio::time::timeout(wait, rx).await {
        Ok(Ok(d)) => Some(d),
        _ => { st.parked.forget(&key); None },
    };
    log::info!("permission {key}: {decision:?}");
    axum::Json(permission_response(decision)).into_response()
}

pub async fn serve(addr: SocketAddr, host: String, token: String, tx: mpsc::Sender<AgentEvent>,
                   parked: Arc<Parked>, wait: Arc<Mutex<Duration>>) -> std::io::Result<SocketAddr> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let bound = listener.local_addr()?;
    let app = router(host, token, tx, parked, wait);
    tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await { log::error!("hooks server: {e}"); }
    });
    Ok(bound)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test] fn responses_follow_the_hook_schema() {
        let allow = permission_response(Some(Decision::Allow));
        assert_eq!(allow["hookSpecificOutput"]["decision"]["behavior"], "allow");
        assert_eq!(allow["hookSpecificOutput"]["hookEventName"], "PermissionRequest");
        assert_eq!(permission_response(Some(Decision::Deny))["hookSpecificOutput"]["decision"]["behavior"], "deny");
        assert_eq!(permission_response(Some(Decision::Terminal)), serde_json::json!({}));
        assert_eq!(permission_response(None), serde_json::json!({}));
        assert_eq!(Decision::parse("allow"), Some(Decision::Allow));
        assert_eq!(Decision::parse("maybe"), None);
    }

    #[tokio::test] async fn parked_requests_resolve_once_and_newer_replaces_older() {
        let p = Parked::default();
        let rx1 = p.park("h:s".into());
        assert!(p.is_parked("h:s"));
        let rx2 = p.park("h:s".into());
        assert_eq!(rx1.await.unwrap(), Decision::Terminal, "older request released without a decision");
        assert!(p.answer("h:s", Decision::Allow));
        assert_eq!(rx2.await.unwrap(), Decision::Allow);
        assert!(!p.answer("h:s", Decision::Deny), "nothing left to answer");
        assert!(!p.is_parked("h:s"));
    }
}
