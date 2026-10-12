use buddy_lib::collector::event::EventKind;
use buddy_lib::collector::hooks_server::{serve, Parked};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;

#[tokio::test]
async fn health_auth_and_delivery() {
    let (tx, mut rx) = mpsc::channel(16);
    let addr = serve("127.0.0.1:0".parse().unwrap(), "h".into(), "tok".into(), tx, Arc::new(Parked::default()), Arc::new(Mutex::new(Duration::from_millis(300)))).await.unwrap();
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

#[tokio::test]
async fn permission_request_is_held_then_answered_or_released() {
    let (tx, mut rx) = mpsc::channel(16);
    let parked = Arc::new(Parked::default());
    let addr = serve("127.0.0.1:0".parse().unwrap(), "h".into(), "tok".into(), tx, parked.clone(), Arc::new(Mutex::new(Duration::from_millis(400)))).await.unwrap();
    let c = reqwest::Client::new();
    let body = serde_json::json!({"session_id":"s","hook_event_name":"PermissionRequest","tool_name":"Bash","tool_input":{"command":"rm -rf x"}});
    // answered from the bubble
    let p2 = parked.clone();
    let answer = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(p2.is_parked("h:s"));
        p2.answer("h:s", buddy_lib::collector::hooks_server::Decision::Allow)
    });
    let r = c.post(format!("http://{addr}/hook")).bearer_auth("tok").json(&body).send().await.unwrap();
    assert_eq!(r.status(), 200);
    let v: serde_json::Value = r.json().await.unwrap();
    assert_eq!(v["hookSpecificOutput"]["decision"]["behavior"], "allow");
    assert!(answer.await.unwrap());
    assert_eq!(rx.recv().await.unwrap().session_id, "s", "the event still reaches the store");
    // nobody answers: released after the wait with no decision
    let t0 = std::time::Instant::now();
    let r = c.post(format!("http://{addr}/hook")).bearer_auth("tok").json(&body).send().await.unwrap();
    let v: serde_json::Value = r.json().await.unwrap();
    assert_eq!(v, serde_json::json!({}));
    assert!(t0.elapsed() >= Duration::from_millis(350));
    assert!(!parked.is_parked("h:s"));
}
