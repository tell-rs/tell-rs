use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::json;
use tokio::io::AsyncReadExt;
use tokio::net::TcpListener;

use tell::{Events, HistogramParams, LogLevel, Tell, TellConfig, Temporality, props};

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

    let config = TellConfig::builder("feed1e11feed1e11feed1e11feed1e11")
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
        let root_offset =
            u32::from_le_bytes([frames[0][0], frames[0][1], frames[0][2], frames[0][3]]) as usize;
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
            if let Ok(frame) =
                tokio::time::timeout(Duration::from_millis(500), read_frame(&mut stream)).await
            {
                received_clone.lock().unwrap().push(frame);
            } else {
                break;
            }
        }
    });

    let config = TellConfig::builder("feed1e11feed1e11feed1e11feed1e11")
        .endpoint(addr.to_string())
        .batch_size(100)
        .flush_interval(Duration::from_secs(60))
        .build()
        .unwrap();

    let client = Tell::new(config).unwrap();

    client.identify("user_456", Some(json!({"name": "Jane", "plan": "pro"})));
    client.identify("user_789", None::<serde_json::Value>); // no traits — hits else branch
    client.group("user_456", "org_789", Some(json!({"name": "Acme Corp"})));
    client.group("user_456", "org_789", None::<serde_json::Value>); // no props — hits else branch
    client.flush().await.unwrap();

    tokio::time::sleep(Duration::from_millis(100)).await;

    {
        let frames = received.lock().unwrap();
        assert!(
            !frames.is_empty(),
            "should have received at least one frame"
        );
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

    let config = TellConfig::builder("feed1e11feed1e11feed1e11feed1e11")
        .endpoint(addr.to_string())
        .batch_size(10)
        .flush_interval(Duration::from_secs(60))
        .build()
        .unwrap();

    let client = Tell::new(config).unwrap();

    client.log_error(
        "connection refused",
        Some("api"),
        Some(json!({"code": 500})),
    );
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

    let config = TellConfig::builder("feed1e11feed1e11feed1e11feed1e11")
        .endpoint(addr.to_string())
        .batch_size(10)
        .flush_interval(Duration::from_secs(60))
        .build()
        .unwrap();

    let client = Tell::new(config).unwrap();

    client.revenue(
        "user_1",
        49.99,
        "USD",
        "ord_123",
        Some(json!({"sku": "W100"})),
    );
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

    let config = TellConfig::builder("feed1e11feed1e11feed1e11feed1e11")
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
        assert!(
            f1.contains("app_version"),
            "frame 1: super prop app_version missing"
        );
        assert!(f1.contains("env"), "frame 1: super prop env missing");
        assert!(f1.contains("save"), "frame 1: event prop missing");

        // Frame 2: props!() register added "extra" — has app_version, env, extra
        let f2 = String::from_utf8_lossy(&frames[1]);
        assert!(
            f2.contains("app_version"),
            "frame 2: super prop app_version missing"
        );
        assert!(f2.contains("env"), "frame 2: super prop env missing");
        assert!(f2.contains("extra"), "frame 2: props!() super prop missing");

        // Frame 3: after unregister("env") — has app_version, extra, but NOT env
        let f3 = String::from_utf8_lossy(&frames[2]);
        assert!(
            f3.contains("app_version"),
            "frame 3: app_version should remain"
        );
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

    let config = TellConfig::builder("feed1e11feed1e11feed1e11feed1e11")
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
    client.identify("", None::<serde_json::Value>); // empty userId in identify
    client.group("", "g_1", None::<serde_json::Value>); // empty userId in group
    client.group("u_1", "", None::<serde_json::Value>); // empty groupId
    client.revenue("u_1", 10.0, "", "ord_1", None::<serde_json::Value>); // empty currency
    client.revenue("u_1", 10.0, "USD", "", None::<serde_json::Value>); // empty orderId
    client.log(LogLevel::Info, "", None, None::<serde_json::Value>); // empty log message
    client.gauge("", 1.0, &[]); // empty metric name
    client.gauge_dyn("", 1.0, &[]); // empty metric_dyn name
    client.histogram(
        "",
        HistogramParams {
            count: 1,
            sum: 1.0,
            min: 1.0,
            max: 1.0,
            buckets: vec![],
        },
        &[],
    ); // empty histogram name

    // Give time for validation to run (it's synchronous before send)
    tokio::time::sleep(Duration::from_millis(50)).await;

    {
        let errs = errors.lock().unwrap();
        assert_eq!(
            errs.len(),
            12,
            "expected 12 validation errors, got {:?}",
            *errs
        );
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
            if let Ok(frame) =
                tokio::time::timeout(Duration::from_millis(500), read_frame(&mut stream)).await
            {
                received_clone.lock().unwrap().push(frame);
            } else {
                break;
            }
        }
    });

    let config = TellConfig::builder("feed1e11feed1e11feed1e11feed1e11")
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
        assert_ne!(
            frames[0], frames[1],
            "frames should differ due to new session"
        );
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
        if let Ok(frame) =
            tokio::time::timeout(Duration::from_millis(500), read_frame(&mut stream)).await
        {
            received_clone.lock().unwrap().push(frame);
        }
    });

    let config = TellConfig::builder("feed1e11feed1e11feed1e11feed1e11")
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
        assert_eq!(
            frames.len(),
            1,
            "close should have flushed the pending event"
        );
    }

    server.abort();
}

#[tokio::test]
async fn gauge_sends_metric_batch_to_tcp_server() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let received = Arc::new(Mutex::new(Vec::new()));
    let received_clone = received.clone();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let frame = read_frame(&mut stream).await;
        received_clone.lock().unwrap().push(frame);
    });

    let config = TellConfig::builder("feed1e11feed1e11feed1e11feed1e11")
        .endpoint(addr.to_string())
        .source("test-host")
        .service("test-svc")
        .batch_size(10)
        .flush_interval(Duration::from_secs(60))
        .build()
        .unwrap();

    let client = Tell::new(config).unwrap();

    client.gauge("system.cpu.user", 45.2, &[("core", "0")]);
    client.gauge("system.load.1", 0.75, &[]);
    client.flush().await.unwrap();

    tokio::time::sleep(Duration::from_millis(50)).await;

    {
        let frames = received.lock().unwrap();
        assert_eq!(frames.len(), 1);
        assert!(!frames[0].is_empty());

        // Valid FlatBuffer root
        let root_offset =
            u32::from_le_bytes([frames[0][0], frames[0][1], frames[0][2], frames[0][3]]) as usize;
        assert!(root_offset < frames[0].len());

        // Metric names should appear in the binary
        assert!(
            frames[0]
                .windows(b"system.cpu.user".len())
                .any(|w| w == b"system.cpu.user"),
            "metric name 'system.cpu.user' not found in frame",
        );
        assert!(
            frames[0]
                .windows(b"system.load.1".len())
                .any(|w| w == b"system.load.1"),
            "metric name 'system.load.1' not found in frame",
        );

        // Source hostname should appear
        assert!(
            frames[0]
                .windows(b"test-host".len())
                .any(|w| w == b"test-host"),
            "source 'test-host' not found in frame",
        );

        // Service should appear
        assert!(
            frames[0]
                .windows(b"test-svc".len())
                .any(|w| w == b"test-svc"),
            "service 'test-svc' not found in frame",
        );

        // Label key/value should appear
        assert!(
            frames[0].windows(b"core".len()).any(|w| w == b"core"),
            "label key 'core' not found in frame",
        );
    }

    client.close().await.unwrap();
    server.abort();
}

#[tokio::test]
async fn counter_sends_metric_with_temporality() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let received = Arc::new(Mutex::new(Vec::new()));
    let received_clone = received.clone();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let frame = read_frame(&mut stream).await;
        received_clone.lock().unwrap().push(frame);
    });

    let config = TellConfig::builder("feed1e11feed1e11feed1e11feed1e11")
        .endpoint(addr.to_string())
        .batch_size(10)
        .flush_interval(Duration::from_secs(60))
        .build()
        .unwrap();

    let client = Tell::new(config).unwrap();

    client.counter("system.net.bytes_recv", 98765.0, &[("interface", "eth0")]);
    client.counter_with_temporality(
        "http_requests_total",
        50523.0,
        &[("method", "GET")],
        Temporality::Cumulative,
    );
    client.flush().await.unwrap();

    tokio::time::sleep(Duration::from_millis(50)).await;

    {
        let frames = received.lock().unwrap();
        assert_eq!(frames.len(), 1);

        assert!(
            frames[0]
                .windows(b"system.net.bytes_recv".len())
                .any(|w| w == b"system.net.bytes_recv"),
            "counter name not found in frame",
        );
        assert!(
            frames[0]
                .windows(b"interface".len())
                .any(|w| w == b"interface"),
            "label key 'interface' not found in frame",
        );
        assert!(
            frames[0].windows(b"eth0".len()).any(|w| w == b"eth0"),
            "label value 'eth0' not found in frame",
        );
        assert!(
            frames[0]
                .windows(b"http_requests_total".len())
                .any(|w| w == b"http_requests_total"),
            "cumulative counter name not found in frame",
        );
    }

    client.close().await.unwrap();
    server.abort();
}

#[tokio::test]
async fn alias_sends_event_to_tcp_server() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let received = Arc::new(Mutex::new(Vec::new()));
    let received_clone = received.clone();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let frame = read_frame(&mut stream).await;
        received_clone.lock().unwrap().push(frame);
    });

    let config = TellConfig::builder("feed1e11feed1e11feed1e11feed1e11")
        .endpoint(addr.to_string())
        .batch_size(10)
        .flush_interval(Duration::from_secs(60))
        .build()
        .unwrap();

    let client = Tell::new(config).unwrap();

    client.alias("old_anon_123", "user_456");
    client.flush().await.unwrap();

    tokio::time::sleep(Duration::from_millis(50)).await;

    {
        let frames = received.lock().unwrap();
        assert_eq!(frames.len(), 1);
        assert!(
            frames[0]
                .windows(b"previous_id".len())
                .any(|w| w == b"previous_id"),
            "previous_id not found in frame",
        );
    }

    client.close().await.unwrap();
    server.abort();
}

#[tokio::test]
async fn alias_validation_errors() {
    let errors = Arc::new(Mutex::new(Vec::new()));
    let errors_clone = errors.clone();

    let config = TellConfig::builder("feed1e11feed1e11feed1e11feed1e11")
        .endpoint("127.0.0.1:1")
        .on_error(move |e| {
            errors_clone.lock().unwrap().push(format!("{e}"));
        })
        .build()
        .unwrap();

    let client = Tell::new(config).unwrap();

    client.alias("", "user_1");
    client.alias("old_id", "");

    tokio::time::sleep(Duration::from_millis(50)).await;

    {
        let errs = errors.lock().unwrap();
        assert_eq!(
            errs.len(),
            2,
            "expected 2 alias validation errors, got {:?}",
            *errs
        );
    }

    client.close().await.ok();
}

#[tokio::test]
async fn histogram_sends_metric_to_tcp_server() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let received = Arc::new(Mutex::new(Vec::new()));
    let received_clone = received.clone();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let frame = read_frame(&mut stream).await;
        received_clone.lock().unwrap().push(frame);
    });

    let config = TellConfig::builder("feed1e11feed1e11feed1e11feed1e11")
        .endpoint(addr.to_string())
        .batch_size(10)
        .flush_interval(Duration::from_secs(60))
        .build()
        .unwrap();

    let client = Tell::new(config).unwrap();

    client.histogram(
        "http.request.duration",
        HistogramParams {
            count: 100,
            sum: 5432.1,
            min: 1.0,
            max: 200.0,
            buckets: vec![(10.0, 20), (50.0, 60), (100.0, 90), (f64::INFINITY, 100)],
        },
        &[("method", "GET")],
    );
    client.flush().await.unwrap();

    tokio::time::sleep(Duration::from_millis(50)).await;

    {
        let frames = received.lock().unwrap();
        assert_eq!(frames.len(), 1);
        assert!(
            frames[0]
                .windows(b"http.request.duration".len())
                .any(|w| w == b"http.request.duration"),
            "histogram name not found in frame",
        );
    }

    client.close().await.unwrap();
    server.abort();
}

#[tokio::test]
async fn gauge_dyn_and_counter_dyn_send_metrics() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let received = Arc::new(Mutex::new(Vec::new()));
    let received_clone = received.clone();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let frame = read_frame(&mut stream).await;
        received_clone.lock().unwrap().push(frame);
    });

    let config = TellConfig::builder("feed1e11feed1e11feed1e11feed1e11")
        .endpoint(addr.to_string())
        .batch_size(10)
        .flush_interval(Duration::from_secs(60))
        .build()
        .unwrap();

    let client = Tell::new(config).unwrap();

    let dynamic_host = String::from("web-01");
    client.gauge_dyn("system.memory", 75.5, &[("host", dynamic_host.as_str())]);
    client.counter_dyn("http.requests", 42.0, &[("host", dynamic_host.as_str())]);
    client.flush().await.unwrap();

    tokio::time::sleep(Duration::from_millis(50)).await;

    {
        let frames = received.lock().unwrap();
        assert_eq!(frames.len(), 1);
        assert!(
            frames[0]
                .windows(b"system.memory".len())
                .any(|w| w == b"system.memory"),
            "gauge_dyn metric not found",
        );
        assert!(
            frames[0]
                .windows(b"http.requests".len())
                .any(|w| w == b"http.requests"),
            "counter_dyn metric not found",
        );
        assert!(
            frames[0].windows(b"web-01".len()).any(|w| w == b"web-01"),
            "dynamic label value not found",
        );
    }

    client.close().await.unwrap();
    server.abort();
}

#[tokio::test]
async fn log_convenience_methods_all_levels() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let received = Arc::new(Mutex::new(Vec::new()));
    let received_clone = received.clone();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let frame = read_frame(&mut stream).await;
        received_clone.lock().unwrap().push(frame);
    });

    let config = TellConfig::builder("feed1e11feed1e11feed1e11feed1e11")
        .endpoint(addr.to_string())
        .batch_size(100)
        .flush_interval(Duration::from_secs(60))
        .build()
        .unwrap();

    let client = Tell::new(config).unwrap();

    client.log_emergency("emerg_msg", Some("kern"), None::<serde_json::Value>);
    client.log_alert("alert_msg", Some("auth"), None::<serde_json::Value>);
    client.log_critical("crit_msg", None, None::<serde_json::Value>);
    client.log_warning("warn_msg", None, None::<serde_json::Value>);
    client.log_notice("notice_msg", None, None::<serde_json::Value>);
    client.log_info("info_msg", None, None::<serde_json::Value>);
    client.log_debug("debug_msg", None, None::<serde_json::Value>);
    client.log_trace("trace_msg", None, None::<serde_json::Value>);
    client.flush().await.unwrap();

    tokio::time::sleep(Duration::from_millis(50)).await;

    {
        let frames = received.lock().unwrap();
        assert_eq!(frames.len(), 1);
        for msg in &[
            "emerg_msg",
            "alert_msg",
            "crit_msg",
            "warn_msg",
            "notice_msg",
            "info_msg",
            "debug_msg",
            "trace_msg",
        ] {
            assert!(
                frames[0].windows(msg.len()).any(|w| w == msg.as_bytes()),
                "log message '{}' not found in frame",
                msg,
            );
        }
    }

    client.close().await.unwrap();
    server.abort();
}

#[tokio::test]
async fn revenue_with_super_properties() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let received = Arc::new(Mutex::new(Vec::new()));
    let received_clone = received.clone();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let frame = read_frame(&mut stream).await;
        received_clone.lock().unwrap().push(frame);
    });

    let config = TellConfig::builder("feed1e11feed1e11feed1e11feed1e11")
        .endpoint(addr.to_string())
        .batch_size(10)
        .flush_interval(Duration::from_secs(60))
        .build()
        .unwrap();

    let client = Tell::new(config).unwrap();

    client.register(json!({"source": "web"}));
    client.revenue(
        "user_1",
        29.99,
        "EUR",
        "ord_456",
        Some(json!({"item": "widget"})),
    );
    client.flush().await.unwrap();

    tokio::time::sleep(Duration::from_millis(50)).await;

    {
        let frames = received.lock().unwrap();
        assert_eq!(frames.len(), 1);
        let f = String::from_utf8_lossy(&frames[0]);
        assert!(f.contains("Order Completed"), "event name missing");
        assert!(f.contains("source"), "super prop 'source' missing");
        assert!(f.contains("EUR"), "currency missing");
        assert!(f.contains("widget"), "prop 'item' missing");
    }

    client.close().await.unwrap();
    server.abort();
}

#[tokio::test]
async fn group_with_super_properties() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let received = Arc::new(Mutex::new(Vec::new()));
    let received_clone = received.clone();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let frame = read_frame(&mut stream).await;
        received_clone.lock().unwrap().push(frame);
    });

    let config = TellConfig::builder("feed1e11feed1e11feed1e11feed1e11")
        .endpoint(addr.to_string())
        .batch_size(10)
        .flush_interval(Duration::from_secs(60))
        .build()
        .unwrap();

    let client = Tell::new(config).unwrap();

    client.register(json!({"platform": "ios"}));
    client.group("user_1", "org_123", Some(json!({"org_name": "Acme"})));
    client.flush().await.unwrap();

    tokio::time::sleep(Duration::from_millis(50)).await;

    {
        let frames = received.lock().unwrap();
        assert_eq!(frames.len(), 1);
        let f = String::from_utf8_lossy(&frames[0]);
        assert!(f.contains("platform"), "super prop 'platform' missing");
        assert!(f.contains("ios"), "super prop value missing");
        assert!(f.contains("org_123"), "group_id missing");
        assert!(f.contains("Acme"), "prop 'org_name' missing");
    }

    client.close().await.unwrap();
    server.abort();
}

#[tokio::test]
async fn timer_tick_flushes_automatically() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let received = Arc::new(Mutex::new(Vec::new()));
    let received_clone = received.clone();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let frame = read_frame(&mut stream).await;
        received_clone.lock().unwrap().push(frame);
    });

    let config = TellConfig::builder("feed1e11feed1e11feed1e11feed1e11")
        .endpoint(addr.to_string())
        .batch_size(1000) // won't hit size threshold
        .flush_interval(Duration::from_millis(100)) // short timer
        .build()
        .unwrap();

    let client = Tell::new(config).unwrap();

    client.track("u_1", "Timer Event", None::<serde_json::Value>);

    // Don't call flush — let the timer tick handle it
    tokio::time::sleep(Duration::from_millis(300)).await;

    {
        let frames = received.lock().unwrap();
        assert_eq!(
            frames.len(),
            1,
            "timer tick should have auto-flushed the event"
        );
    }

    client.close().await.unwrap();
    server.abort();
}

#[tokio::test]
async fn batch_size_triggers_log_flush() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let received = Arc::new(Mutex::new(Vec::new()));
    let received_clone = received.clone();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let frame = read_frame(&mut stream).await;
        received_clone.lock().unwrap().push(frame);
    });

    let config = TellConfig::builder("feed1e11feed1e11feed1e11feed1e11")
        .endpoint(addr.to_string())
        .batch_size(1) // flush after 1 log
        .flush_interval(Duration::from_secs(60))
        .build()
        .unwrap();

    let client = Tell::new(config).unwrap();

    client.log(
        LogLevel::Info,
        "batch trigger",
        None,
        None::<serde_json::Value>,
    );

    // batch_size=1 should trigger immediate flush without calling flush()
    tokio::time::sleep(Duration::from_millis(200)).await;

    {
        let frames = received.lock().unwrap();
        assert_eq!(frames.len(), 1, "batch_size should trigger log flush");
    }

    client.close().await.unwrap();
    server.abort();
}

#[tokio::test]
async fn counter_dyn_with_temporality_sends_cumulative() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let received = Arc::new(Mutex::new(Vec::new()));
    let received_clone = received.clone();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let frame = read_frame(&mut stream).await;
        received_clone.lock().unwrap().push(frame);
    });

    let config = TellConfig::builder("feed1e11feed1e11feed1e11feed1e11")
        .endpoint(addr.to_string())
        .batch_size(10)
        .flush_interval(Duration::from_secs(60))
        .build()
        .unwrap();

    let client = Tell::new(config).unwrap();

    let iface = String::from("eth0");
    client.counter_dyn_with_temporality(
        "system.net.bytes_recv",
        5_000_000.0,
        &[("interface", iface.as_str())],
        Temporality::Cumulative,
    );
    let device = String::from("sda");
    client.counter_dyn_with_temporality(
        "system.disk.read_bytes",
        12_345_678.0,
        &[("device", device.as_str())],
        Temporality::Cumulative,
    );
    client.flush().await.unwrap();

    tokio::time::sleep(Duration::from_millis(50)).await;

    {
        let frames = received.lock().unwrap();
        assert_eq!(frames.len(), 1);

        assert!(
            frames[0]
                .windows(b"system.net.bytes_recv".len())
                .any(|w| w == b"system.net.bytes_recv"),
            "cumulative metric name not found in frame",
        );
        assert!(
            frames[0]
                .windows(b"system.disk.read_bytes".len())
                .any(|w| w == b"system.disk.read_bytes"),
            "cumulative disk metric not found in frame",
        );
        assert!(
            frames[0].windows(b"eth0".len()).any(|w| w == b"eth0"),
            "dynamic label value 'eth0' not found",
        );
        assert!(
            frames[0].windows(b"sda".len()).any(|w| w == b"sda"),
            "dynamic label value 'sda' not found",
        );
    }

    client.close().await.unwrap();
    server.abort();
}

#[tokio::test]
async fn batch_size_triggers_metric_flush() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let received = Arc::new(Mutex::new(Vec::new()));
    let received_clone = received.clone();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let frame = read_frame(&mut stream).await;
        received_clone.lock().unwrap().push(frame);
    });

    let config = TellConfig::builder("feed1e11feed1e11feed1e11feed1e11")
        .endpoint(addr.to_string())
        .batch_size(1) // flush after 1 metric
        .flush_interval(Duration::from_secs(60))
        .build()
        .unwrap();

    let client = Tell::new(config).unwrap();

    client.gauge("cpu.usage", 55.0, &[]);

    // batch_size=1 should trigger immediate flush
    tokio::time::sleep(Duration::from_millis(200)).await;

    {
        let frames = received.lock().unwrap();
        assert_eq!(frames.len(), 1, "batch_size should trigger metric flush");
    }

    client.close().await.unwrap();
    server.abort();
}

#[tokio::test]
async fn retry_exhaustion_falls_back_to_disk_buffer() {
    let dir = std::env::temp_dir().join(format!("tell-wal-retry-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);

    let errors = Arc::new(Mutex::new(Vec::new()));
    let errors_clone = errors.clone();

    // Connect to a port with no listener — all sends will fail
    let config = TellConfig::builder("feed1e11feed1e11feed1e11feed1e11")
        .endpoint("127.0.0.1:1")
        .max_retries(0) // fail immediately
        .batch_size(10)
        .flush_interval(Duration::from_secs(60))
        .network_timeout(Duration::from_millis(100))
        .buffer_path(&dir)
        .buffer_max_bytes(1024 * 1024)
        .on_error(move |e| {
            errors_clone.lock().unwrap().push(format!("{e}"));
        })
        .build()
        .unwrap();

    let client = Tell::new(config).unwrap();

    client.track("u_1", "WAL Event", None::<serde_json::Value>);
    client.flush().await.ok();

    // Give worker time to retry and fall back to WAL
    tokio::time::sleep(Duration::from_millis(500)).await;

    client.close().await.ok();

    // WAL file should exist and contain data
    let wal_path = dir.join("buffer.wal");
    assert!(wal_path.exists(), "WAL file should have been created");
    let wal_size = std::fs::metadata(&wal_path).unwrap().len();
    assert!(wal_size > 0, "WAL should contain buffered data");

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn disk_buffer_drained_on_reconnect() {
    let dir = std::env::temp_dir().join(format!("tell-wal-drain-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);

    // Phase 1: write to WAL by sending to a dead port
    {
        let config = TellConfig::builder("feed1e11feed1e11feed1e11feed1e11")
            .endpoint("127.0.0.1:1")
            .max_retries(0)
            .batch_size(10)
            .flush_interval(Duration::from_secs(60))
            .network_timeout(Duration::from_millis(100))
            .buffer_path(&dir)
            .buffer_max_bytes(1024 * 1024)
            .build()
            .unwrap();

        let client = Tell::new(config).unwrap();
        client.track("u_1", "Buffered Event", None::<serde_json::Value>);
        client.flush().await.ok();
        tokio::time::sleep(Duration::from_millis(500)).await;
        client.close().await.ok();
    }

    // Verify WAL has data
    let wal_size = std::fs::metadata(dir.join("buffer.wal")).unwrap().len();
    assert!(wal_size > 0, "WAL should have data from phase 1");

    // Phase 2: start a real server and new client with same buffer_path
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let received = Arc::new(Mutex::new(Vec::new()));
    let received_clone = received.clone();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        // Read up to 3 frames (WAL drain + any new events)
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

    let config = TellConfig::builder("feed1e11feed1e11feed1e11feed1e11")
        .endpoint(addr.to_string())
        .batch_size(10)
        .flush_interval(Duration::from_millis(100)) // short tick to trigger WAL drain
        .buffer_path(&dir)
        .buffer_max_bytes(1024 * 1024)
        .build()
        .unwrap();

    let client = Tell::new(config).unwrap();

    // Wait for timer tick to drain WAL
    tokio::time::sleep(Duration::from_millis(500)).await;

    {
        let frames = received.lock().unwrap();
        assert!(
            !frames.is_empty(),
            "WAL data should have been drained to the server"
        );
    }

    client.close().await.unwrap();
    server.abort();

    let _ = std::fs::remove_dir_all(&dir);
}
