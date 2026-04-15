use super::*;

// --- R1: Default config sends no session id ---

#[tokio::test]
async fn test_track_default_config_session_none() {
    let (listener, client) = setup_no_session().await;
    let server = tokio::spawn(recv_one_frame(listener));

    client.track("user_1", "Test Event", None::<serde_json::Value>);
    client.flush().await.ok();

    let frame = server.await.unwrap();
    assert!(
        !bytes_contain(&frame, &SID_A),
        "default config track must not embed the SID_A sentinel in the frame"
    );
    client.close().await.ok();
}

#[tokio::test]
async fn test_identify_default_config_session_none() {
    let (listener, client) = setup_no_session().await;
    let server = tokio::spawn(recv_one_frame(listener));

    client.identify("user_1", None::<serde_json::Value>);
    client.flush().await.ok();

    let frame = server.await.unwrap();
    assert!(
        !bytes_contain(&frame, &SID_A),
        "default config identify must not embed session sentinel"
    );
    client.close().await.ok();
}

#[tokio::test]
async fn test_alias_default_config_session_none() {
    let (listener, client) = setup_no_session().await;
    let server = tokio::spawn(recv_one_frame(listener));

    client.alias("anon_99", "user_1");
    client.flush().await.ok();

    let frame = server.await.unwrap();
    assert!(
        !bytes_contain(&frame, &SID_A),
        "default config alias must not embed session sentinel"
    );
    client.close().await.ok();
}

#[tokio::test]
async fn test_group_default_config_session_none() {
    let (listener, client) = setup_no_session().await;
    let server = tokio::spawn(recv_one_frame(listener));

    client.group("user_1", "org_42", None::<serde_json::Value>);
    client.flush().await.ok();

    let frame = server.await.unwrap();
    assert!(
        !bytes_contain(&frame, &SID_A),
        "default config group must not embed session sentinel"
    );
    client.close().await.ok();
}

#[tokio::test]
async fn test_revenue_default_config_session_none() {
    let (listener, client) = setup_no_session().await;
    let server = tokio::spawn(recv_one_frame(listener));

    client.revenue(
        "user_1",
        9.99,
        "USD",
        "order_001",
        None::<serde_json::Value>,
    );
    client.flush().await.ok();

    let frame = server.await.unwrap();
    assert!(
        !bytes_contain(&frame, &SID_A),
        "default config revenue must not embed session sentinel"
    );
    client.close().await.ok();
}

#[tokio::test]
async fn test_log_info_default_config_session_none() {
    let (listener, client) = setup_no_session().await;
    let server = tokio::spawn(recv_one_frame(listener));

    client.log_info("heartbeat", None, None::<serde_json::Value>);
    client.flush().await.ok();

    let frame = server.await.unwrap();
    assert!(
        !bytes_contain(&frame, &SID_A),
        "default config log_info must not embed session sentinel"
    );
    client.close().await.ok();
}

#[tokio::test]
async fn test_log_error_default_config_session_none() {
    let (listener, client) = setup_no_session().await;
    let server = tokio::spawn(recv_one_frame(listener));

    client.log_error("boom", None, None::<serde_json::Value>);
    client.flush().await.ok();

    let frame = server.await.unwrap();
    assert!(
        !bytes_contain(&frame, &SID_A),
        "default config log_error must not embed session sentinel"
    );
    client.close().await.ok();
}

// --- R2: enable_session behavior ---

#[tokio::test]
async fn test_enable_session_two_clients_distinct_ids() {
    let (listener_a, client_a) = setup_with_session().await;
    let server_a = tokio::spawn(recv_one_frame(listener_a));
    client_a.track_with_session(&SID_A, "user_a", "Event", None::<serde_json::Value>);
    client_a.flush().await.ok();
    let frame_a = server_a.await.unwrap();
    client_a.close().await.ok();

    let (listener_b, client_b) = setup_with_session().await;
    let server_b = tokio::spawn(recv_one_frame(listener_b));
    client_b.track("user_b", "Event", None::<serde_json::Value>);
    client_b.flush().await.ok();
    let frame_b = server_b.await.unwrap();
    client_b.close().await.ok();

    assert!(
        bytes_contain(&frame_a, &SID_A),
        "frame_a must contain SID_A (was sent with track_with_session)"
    );
    assert!(
        !bytes_contain(&frame_b, &SID_A),
        "two enable_session clients must carry distinct session ids; SID_A must not appear in client B's frame"
    );
}

#[tokio::test]
async fn test_enable_session_track_stamps_stable_id() {
    let (listener, client) = setup_with_session().await;

    let auto_sid = client
        .current_session_id()
        .expect("enable_session must produce a non-None session id");

    let server = tokio::spawn(recv_one_frame(listener));

    client.track("user_1", "Event1", None::<serde_json::Value>);
    client.track("user_1", "Event2", None::<serde_json::Value>);
    client.flush().await.ok();

    let frame = server.await.unwrap();

    assert!(
        bytes_contain(&frame, &auto_sid),
        "enable_session must stamp the auto-session id on track events"
    );

    client.close().await.ok();
}

// --- R3: Identity messages never stamp session even with enable_session ---

#[tokio::test]
async fn test_identify_never_stamps_session_with_enable_session() {
    let (listener, client) = setup_with_session().await;

    let auto_sid = client
        .current_session_id()
        .expect("enable_session must produce a non-None session id");

    let server = tokio::spawn(recv_one_frame(listener));

    client.identify("user_1", None::<serde_json::Value>);
    client.flush().await.ok();

    let frame = server.await.unwrap();

    assert!(
        !bytes_contain(&frame, &auto_sid),
        "identify must not stamp session_id; auto_sid must be absent from identify frame"
    );
    assert!(
        bytes_contain(&frame, b"user_1"),
        "identify frame must still contain user_id"
    );

    client.close().await.ok();
}

#[tokio::test]
async fn test_alias_never_stamps_session_with_enable_session() {
    let (listener, client) = setup_with_session().await;

    let auto_sid = client
        .current_session_id()
        .expect("enable_session must produce a non-None session id");

    let server = tokio::spawn(recv_one_frame(listener));

    client.alias("anon_99", "user_1");
    client.flush().await.ok();

    let frame = server.await.unwrap();

    assert!(
        !bytes_contain(&frame, &auto_sid),
        "alias must not stamp session_id; auto_sid must be absent from alias frame"
    );
    assert!(
        bytes_contain(&frame, b"anon_99"),
        "alias frame must contain previous_id"
    );

    client.close().await.ok();
}

#[tokio::test]
async fn test_group_never_stamps_session_with_enable_session() {
    let (listener, client) = setup_with_session().await;

    let auto_sid = client
        .current_session_id()
        .expect("enable_session must produce a non-None session id");

    let server = tokio::spawn(recv_one_frame(listener));

    client.group("user_1", "org_42", None::<serde_json::Value>);
    client.flush().await.ok();

    let frame = server.await.unwrap();

    assert!(
        !bytes_contain(&frame, &auto_sid),
        "group must not stamp session_id; auto_sid must be absent from group frame"
    );
    assert!(
        bytes_contain(&frame, b"org_42"),
        "group frame must contain group_id"
    );

    client.close().await.ok();
}
