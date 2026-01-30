use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::json;
use tokio::io::AsyncReadExt;
use tokio::net::TcpListener;

use tell::{props, Tell, TellConfig, Events};

/// Read one length-prefixed frame from a TCP stream.
async fn read_frame(stream: &mut tokio::net::TcpStream) -> Vec<u8> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await.unwrap();
    let len = u32::from_be_bytes(len_buf) as usize;
    let mut payload = vec![0u8; len];
    stream.read_exact(&mut payload).await.unwrap();
    payload
}

#[tokio::test]
async fn track_sends_event_batch_to_tcp_server() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let received = Arc::new(Mutex::new(Vec::new()));
    let received_clone = received.clone();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let frame = read_frame(&mut stream).await;
        received_clone.lock().unwrap().push(frame);
    });

    let config = TellConfig::builder("a1b2c3d4e5f60718293a4b5c6d7e8f90")
        .endpoint(addr.to_string())
        .batch_size(10)
        .flush_interval(Duration::from_secs(60))
        .build()
        .unwrap();

    let client = Tell::new(config).unwrap();

    client.track("user_123", "Page Viewed", Some(json!({"url": "/home"})));
    client.flush().await.unwrap();

    // Give server time to receive
    tokio::time::sleep(Duration::from_millis(50)).await;

    {
        let frames = received.lock().unwrap();
        assert_eq!(frames.len(), 1);
        assert!(!frames[0].is_empty());

        // Verify it's a valid FlatBuffer (root offset is valid)
        let root_offset = u32::from_le_bytes([
            frames[0][0],
            frames[0][1],
            frames[0][2],
            frames[0][3],
        ]) as usize;
        assert!(root_offset < frames[0].len());
    }

    client.close().await.unwrap();
    server.abort();
}

#[tokio::test]
async fn identify_and_group_send_events() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let received = Arc::new(Mutex::new(Vec::new()));
    let received_clone = received.clone();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        for _ in 0..2 {
            if let Ok(frame) = tokio::time::timeout(Duration::from_millis(500), read_frame(&mut stream)).await {
                received_clone.lock().unwrap().push(frame);
            } else {
                break;
            }
        }
    });

    let config = TellConfig::builder("a1b2c3d4e5f60718293a4b5c6d7e8f90")
        .endpoint(addr.to_string())
        .batch_size(100)
        .flush_interval(Duration::from_secs(60))
        .build()
        .unwrap();

    let client = Tell::new(config).unwrap();

    client.identify("user_456", Some(json!({"name": "Jane", "plan": "pro"})));
    client.group("user_456", "org_789", Some(json!({"name": "Acme Corp"})));
    client.flush().await.unwrap();

    tokio::time::sleep(Duration::from_millis(100)).await;

    {
        let frames = received.lock().unwrap();
        assert!(!frames.is_empty(), "should have received at least one frame");
    }

    client.close().await.unwrap();
    server.abort();
}

#[tokio::test]
async fn log_sends_to_server() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let received = Arc::new(Mutex::new(Vec::new()));
    let received_clone = received.clone();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let frame = read_frame(&mut stream).await;
        received_clone.lock().unwrap().push(frame);
    });

    let config = TellConfig::builder("a1b2c3d4e5f60718293a4b5c6d7e8f90")
        .endpoint(addr.to_string())
        .batch_size(10)
        .flush_interval(Duration::from_secs(60))
        .build()
        .unwrap();

    let client = Tell::new(config).unwrap();

    client.log_error("connection refused", Some("api"), Some(json!({"code": 500})));
    client.flush().await.unwrap();

    tokio::time::sleep(Duration::from_millis(50)).await;

    {
        let frames = received.lock().unwrap();
        assert_eq!(frames.len(), 1);
    }

    client.close().await.unwrap();
    server.abort();
}

#[tokio::test]
async fn revenue_sends_order_completed() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let received = Arc::new(Mutex::new(Vec::new()));
    let received_clone = received.clone();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let frame = read_frame(&mut stream).await;
        received_clone.lock().unwrap().push(frame);
    });

    let config = TellConfig::builder("a1b2c3d4e5f60718293a4b5c6d7e8f90")
        .endpoint(addr.to_string())
        .batch_size(10)
        .flush_interval(Duration::from_secs(60))
        .build()
        .unwrap();

    let client = Tell::new(config).unwrap();

    client.revenue("user_1", 49.99, "USD", "ord_123", None::<serde_json::Value>);
    client.flush().await.unwrap();

    tokio::time::sleep(Duration::from_millis(50)).await;

    {
        let frames = received.lock().unwrap();
        assert_eq!(frames.len(), 1);

        // The "Order Completed" event name should be in the binary
        let found = frames[0]
            .windows(b"Order Completed".len())
            .any(|w| w == b"Order Completed");
        assert!(found, "Order Completed event name not found in frame");
    }

    client.close().await.unwrap();
    server.abort();
}

#[tokio::test]
async fn super_properties_merged() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let received = Arc::new(Mutex::new(Vec::new()));
    let received_clone = received.clone();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        for _ in 0..3 {
            if let Ok(frame) =
                tokio::time::timeout(Duration::from_millis(500), read_frame(&mut stream)).await
            {
                received_clone.lock().unwrap().push(frame);
            } else {
                break;
            }
        }
    });

    let config = TellConfig::builder("a1b2c3d4e5f60718293a4b5c6d7e8f90")
        .endpoint(addr.to_string())
        .batch_size(1) // one event per batch for clean frame separation
        .flush_interval(Duration::from_secs(60))
        .build()
        .unwrap();

    let client = Tell::new(config).unwrap();

    // Register with json!() (Serialize path)
    client.register(json!({"app_version": "2.0", "env": "prod"}));
    client.track("user_1", "Event1", Some(json!({"button": "save"})));
    client.flush().await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Register with props!() (IntoPayload path)
    client.register(props! { "extra" => "value" });
    client.track("user_1", "Event2", None::<serde_json::Value>);
    client.flush().await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Unregister "env", track again — should lack "env" but keep "app_version" and "extra"
    client.unregister("env");
    client.track("user_1", "Event3", None::<serde_json::Value>);
    client.flush().await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    {
        let frames = received.lock().unwrap();
        assert_eq!(frames.len(), 3, "expected 3 frames");

        // Frame 1: json!() register — has app_version, env, button
        let f1 = String::from_utf8_lossy(&frames[0]);
        assert!(f1.contains("app_version"), "frame 1: super prop app_version missing");
        assert!(f1.contains("env"), "frame 1: super prop env missing");
        assert!(f1.contains("save"), "frame 1: event prop missing");

        // Frame 2: props!() register added "extra" — has app_version, env, extra
        let f2 = String::from_utf8_lossy(&frames[1]);
        assert!(f2.contains("app_version"), "frame 2: super prop app_version missing");
        assert!(f2.contains("env"), "frame 2: super prop env missing");
        assert!(f2.contains("extra"), "frame 2: props!() super prop missing");

        // Frame 3: after unregister("env") — has app_version, extra, but NOT env
        let f3 = String::from_utf8_lossy(&frames[2]);
        assert!(f3.contains("app_version"), "frame 3: app_version should remain");
        assert!(f3.contains("extra"), "frame 3: extra should remain");
        assert!(!f3.contains("prod"), "frame 3: env value should be removed");
    }

    client.close().await.unwrap();
    server.abort();
}

#[tokio::test]
async fn validation_errors_reported_via_callback() {
    let errors = Arc::new(Mutex::new(Vec::new()));
    let errors_clone = errors.clone();

    let config = TellConfig::builder("a1b2c3d4e5f60718293a4b5c6d7e8f90")
        .endpoint("127.0.0.1:1") // won't actually connect for validation errors
        .on_error(move |e| {
            errors_clone.lock().unwrap().push(format!("{e}"));
        })
        .build()
        .unwrap();

    let client = Tell::new(config).unwrap();

    client.track("", "Event", None::<serde_json::Value>); // empty userId
    client.track("u_1", "", None::<serde_json::Value>); // empty event name
    client.revenue("u_1", -1.0, "USD", "ord_1", None::<serde_json::Value>); // negative amount

    // Give time for validation to run (it's synchronous before send)
    tokio::time::sleep(Duration::from_millis(50)).await;

    {
        let errs = errors.lock().unwrap();
        assert_eq!(errs.len(), 3, "expected 3 validation errors, got {:?}", *errs);
    }

    client.close().await.ok();
}

#[tokio::test]
async fn reset_session_changes_id() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let received = Arc::new(Mutex::new(Vec::new()));
    let received_clone = received.clone();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        for _ in 0..2 {
            if let Ok(frame) = tokio::time::timeout(Duration::from_millis(500), read_frame(&mut stream)).await {
                received_clone.lock().unwrap().push(frame);
            } else {
                break;
            }
        }
    });

    let config = TellConfig::builder("a1b2c3d4e5f60718293a4b5c6d7e8f90")
        .endpoint(addr.to_string())
        .batch_size(1)
        .flush_interval(Duration::from_secs(60))
        .build()
        .unwrap();

    let client = Tell::new(config).unwrap();

    client.track("u_1", "Event 1", None::<serde_json::Value>);
    client.flush().await.unwrap();

    client.reset_session();

    client.track("u_1", "Event 2", None::<serde_json::Value>);
    client.flush().await.unwrap();

    tokio::time::sleep(Duration::from_millis(100)).await;

    {
        let frames = received.lock().unwrap();
        assert_eq!(frames.len(), 2, "expected 2 frames");
        // The frames should contain different session IDs (binary, so we just verify they differ)
        assert_ne!(frames[0], frames[1], "frames should differ due to new session");
    }

    client.close().await.unwrap();
    server.abort();
}

#[tokio::test]
async fn predefined_events_constants() {
    assert_eq!(Events::USER_SIGNED_UP, "User Signed Up");
    assert_eq!(Events::ORDER_COMPLETED, "Order Completed");
    assert_eq!(Events::PAGE_VIEWED, "Page Viewed");
    assert_eq!(Events::FEATURE_USED, "Feature Used");
    assert_eq!(Events::SUBSCRIPTION_STARTED, "Subscription Started");
}

#[tokio::test]
async fn close_flushes_remaining() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let received = Arc::new(Mutex::new(Vec::new()));
    let received_clone = received.clone();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        if let Ok(frame) = tokio::time::timeout(Duration::from_millis(500), read_frame(&mut stream)).await {
            received_clone.lock().unwrap().push(frame);
        }
    });

    let config = TellConfig::builder("a1b2c3d4e5f60718293a4b5c6d7e8f90")
        .endpoint(addr.to_string())
        .batch_size(1000) // won't hit size threshold
        .flush_interval(Duration::from_secs(60)) // won't hit time threshold
        .build()
        .unwrap();

    let client = Tell::new(config).unwrap();

    client.track("u_1", "Test Event", None::<serde_json::Value>);

    // close() should flush
    client.close().await.unwrap();

    tokio::time::sleep(Duration::from_millis(100)).await;

    {
        let frames = received.lock().unwrap();
        assert_eq!(frames.len(), 1, "close should have flushed the pending event");
    }

    server.abort();
}
