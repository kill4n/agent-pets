use buddy_lib::collector::event::EventKind;
use buddy_lib::collector::hooks_server::serve;
use tokio::sync::mpsc;

#[tokio::test]
async fn health_auth_and_delivery() {
    let (tx, mut rx) = mpsc::channel(16);
    let addr = serve("127.0.0.1:0".parse().unwrap(), "h".into(), "tok".into(), tx).await.unwrap();
    let base = format!("http://{addr}");
    let c = reqwest::Client::new();

    assert_eq!(c.get(format!("{base}/health")).send().await.unwrap().status(), 200);

    let r = c.post(format!("{base}/hook")).json(&serde_json::json!({"session_id":"s","hook_event_name":"Stop"}))
        .send().await.unwrap();
    assert_eq!(r.status(), 401);

    let r = c.post(format!("{base}/hook")).bearer_auth("wrong")
        .json(&serde_json::json!({"session_id":"s","hook_event_name":"Stop"})).send().await.unwrap();
    assert_eq!(r.status(), 401);

    let r = c.post(format!("{base}/hook")).bearer_auth("tok")
        .json(&serde_json::json!({"session_id":"s","hook_event_name":"Stop","cwd":"/x"})).send().await.unwrap();
    assert_eq!(r.status(), 200);
    assert_eq!(r.text().await.unwrap(), "");
    let ev = rx.recv().await.unwrap();
    assert_eq!(ev.session_id, "s");
    assert!(matches!(ev.kind, EventKind::Hook(h) if h.event == "Stop"));

    // malformed body: still 200, nothing delivered
    let r = c.post(format!("{base}/hook")).bearer_auth("tok").body("not json")
        .header("content-type", "application/json").send().await.unwrap();
    assert_eq!(r.status(), 200);
    let r = c.post(format!("{base}/hook")).bearer_auth("tok").json(&serde_json::json!({"no":"ids"})).send().await.unwrap();
    assert_eq!(r.status(), 200);
    assert!(tokio::time::timeout(std::time::Duration::from_millis(200), rx.recv()).await.is_err());
}
