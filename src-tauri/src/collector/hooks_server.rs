use super::event::{parse_hook, AgentEvent};
use axum::{body::Bytes, extract::State, http::{HeaderMap, StatusCode}, routing::{get, post}, Router};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::mpsc;

#[derive(Clone)]
struct AppState { host: String, token: Arc<String>, tx: mpsc::Sender<AgentEvent> }

pub fn router(host: String, token: String, tx: mpsc::Sender<AgentEvent>) -> Router {
    Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/hook", post(hook))
        .with_state(AppState { host, token: Arc::new(token), tx })
}

async fn hook(State(st): State<AppState>, headers: HeaderMap, body: Bytes) -> StatusCode {
    let expected = format!("Bearer {}", st.token);
    let ok = headers.get("authorization").and_then(|v| v.to_str().ok()) == Some(expected.as_str());
    if !ok { return StatusCode::UNAUTHORIZED; }
    match serde_json::from_slice::<serde_json::Value>(&body) {
        Ok(v) => match parse_hook(&st.host, &v) {
            Ok(ev) => {
                if let crate::collector::event::EventKind::Hook(h) = &ev.kind {
                    log::debug!("hook {} session={} agent={:?} type={:?} tool={:?}", h.event, ev.session_id, h.agent_id, h.agent_type, h.tool_name);
                }
                let _ = st.tx.try_send(ev);
            }
            Err(e) => log::warn!("hook dropped: {e}"),
        },
        Err(e) => log::warn!("hook body not json: {e}"),
    }
    StatusCode::OK
}

pub async fn serve(addr: SocketAddr, host: String, token: String, tx: mpsc::Sender<AgentEvent>)
    -> std::io::Result<SocketAddr> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let bound = listener.local_addr()?;
    let app = router(host, token, tx);
    tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await { log::error!("hooks server: {e}"); }
    });
    Ok(bound)
}
