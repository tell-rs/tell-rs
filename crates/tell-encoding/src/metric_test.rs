use crate::{
    HistogramParams, LabelParam, MetricEntryParams, MetricType, Temporality, UUID_LENGTH,
    encode_metric_data, encode_metric_data_into, encode_metric_entry,
};

#[test]
fn encode_metric_entry_gauge_with_all_fields() {
    let session_id = [0x07u8; UUID_LENGTH];

    let bytes = encode_metric_entry(&MetricEntryParams {
        metric_type: MetricType::Gauge,
        timestamp: 1749587340000000000,
        name: "system.cpu.user",
        value: 45.2,
        source: Some("server-1"),
        service: Some("tell-agent"),
        labels: &[LabelParam {
            key: "core",
            value: "0",
        }],
        temporality: Temporality::Unspecified,
        histogram: None,
        session_id: Some(&session_id),
    });

    let root_offset = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
    assert!(root_offset < bytes.len());

    let table_start = root_offset;

    // metric_type at table+48
    assert_eq!(bytes[table_start + 48], MetricType::Gauge.as_u8());

    // temporality at table+49
    assert_eq!(bytes[table_start + 49], Temporality::Unspecified.as_u8());

    // timestamp at table+32
    let ts = u64::from_le_bytes([
        bytes[table_start + 32],
        bytes[table_start + 33],
        bytes[table_start + 34],
        bytes[table_start + 35],
        bytes[table_start + 36],
        bytes[table_start + 37],
        bytes[table_start + 38],
        bytes[table_start + 39],
    ]);
    assert_eq!(ts, 1749587340000000000);

    // value at table+40
    let val = f64::from_le_bytes([
        bytes[table_start + 40],
        bytes[table_start + 41],
        bytes[table_start + 42],
        bytes[table_start + 43],
        bytes[table_start + 44],
        bytes[table_start + 45],
        bytes[table_start + 46],
        bytes[table_start + 47],
    ]);
    assert!((val - 45.2).abs() < f64::EPSILON);

    // name
    assert!(
        bytes.windows(15).any(|w| w == b"system.cpu.user"),
        "name not found"
    );

    // source
    assert!(
        bytes.windows(8).any(|w| w == b"server-1"),
        "source not found"
    );

    // service
    assert!(
        bytes.windows(10).any(|w| w == b"tell-agent"),
        "service not found"
    );

    // session_id
    assert!(
        bytes.windows(UUID_LENGTH).any(|w| w == session_id),
        "session_id not found"
    );

    // label key and value
    assert!(
        bytes.windows(4).any(|w| w == b"core"),
        "label key 'core' not found"
    );
    // "0" is a single byte, search for it preceded by the length
    let zero_str = b"0";
    assert!(
        bytes.windows(1).any(|w| w == zero_str),
        "label value '0' not found"
    );
}

#[test]
fn encode_metric_entry_counter_delta() {
    let bytes = encode_metric_entry(&MetricEntryParams {
        metric_type: MetricType::Counter,
        timestamp: 1000000000,
        name: "system.net.bytes_recv",
        value: 123456.0,
        source: Some("web-01"),
        service: None,
        labels: &[LabelParam {
            key: "interface",
            value: "eth0",
        }],
        temporality: Temporality::Delta,
        histogram: None,
        session_id: None,
    });

    let root_offset = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
    assert!(root_offset < bytes.len());

    assert_eq!(bytes[root_offset + 48], MetricType::Counter.as_u8());
    assert_eq!(bytes[root_offset + 49], Temporality::Delta.as_u8());

    assert!(
        bytes.windows(21).any(|w| w == b"system.net.bytes_recv"),
        "name not found"
    );
    assert!(
        bytes.windows(9).any(|w| w == b"interface"),
        "label key not found"
    );
    assert!(
        bytes.windows(4).any(|w| w == b"eth0"),
        "label value not found"
    );
}

#[test]
fn encode_metric_entry_minimal() {
    let bytes = encode_metric_entry(&MetricEntryParams {
        metric_type: MetricType::Gauge,
        timestamp: 0,
        name: "test",
        value: 0.0,
        source: None,
        service: None,
        labels: &[],
        temporality: Temporality::Unspecified,
        histogram: None,
        session_id: None,
    });

    let root_offset = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
    assert!(root_offset < bytes.len());
    assert_eq!(bytes[root_offset + 48], MetricType::Gauge.as_u8());

    assert!(bytes.windows(4).any(|w| w == b"test"), "name not found");
}

#[test]
fn encode_metric_entry_multiple_labels() {
    let bytes = encode_metric_entry(&MetricEntryParams {
        metric_type: MetricType::Counter,
        timestamp: 5000,
        name: "http_requests",
        value: 42.0,
        source: None,
        service: Some("api"),
        labels: &[
            LabelParam {
                key: "method",
                value: "GET",
            },
            LabelParam {
                key: "path",
                value: "/api/users",
            },
        ],
        temporality: Temporality::Cumulative,
        histogram: None,
        session_id: None,
    });

    let root_offset = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
    assert!(root_offset < bytes.len());

    assert!(
        bytes.windows(6).any(|w| w == b"method"),
        "label key 'method' not found"
    );
    assert!(
        bytes.windows(3).any(|w| w == b"GET"),
        "label value 'GET' not found"
    );
    assert!(
        bytes.windows(4).any(|w| w == b"path"),
        "label key 'path' not found"
    );
    assert!(
        bytes.windows(10).any(|w| w == b"/api/users"),
        "label value '/api/users' not found"
    );
}

#[test]
fn encode_metric_data_single() {
    let metric = encode_metric_entry(&MetricEntryParams {
        metric_type: MetricType::Gauge,
        timestamp: 1000,
        name: "cpu",
        value: 50.0,
        source: Some("host-1"),
        service: None,
        labels: &[],
        temporality: Temporality::Unspecified,
        histogram: None,
        session_id: None,
    });

    let data = encode_metric_data(&[metric]);

    let root = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
    assert!(root < data.len());

    assert!(
        data.windows(6).any(|w| w == b"host-1"),
        "source not found in metric_data"
    );
}

#[test]
fn encode_metric_data_multiple() {
    let metrics: Vec<Vec<u8>> = (0..3)
        .map(|i| {
            encode_metric_entry(&MetricEntryParams {
                metric_type: MetricType::Gauge,
                timestamp: 3000 + i,
                name: &format!("metric_{i}"),
                value: i as f64 * 10.0,
                source: Some(&format!("host-{i}")),
                service: Some("svc"),
                labels: &[],
                temporality: Temporality::Unspecified,
                histogram: None,
                session_id: None,
            })
        })
        .collect();

    let data = encode_metric_data(&metrics);

    let root = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
    assert!(root < data.len());

    for i in 0..3 {
        let name = format!("metric_{i}");
        assert!(
            data.windows(name.len()).any(|w| w == name.as_bytes()),
            "metric name '{}' not found in metric_data",
            name,
        );
        let host = format!("host-{i}");
        assert!(
            data.windows(host.len()).any(|w| w == host.as_bytes()),
            "source '{}' not found in metric_data",
            host,
        );
    }
}

#[test]
fn encode_metric_data_into_matches_encode_metric_data() {
    let sources: Vec<String> = (0..3).map(|i| format!("host-{i}")).collect();
    let names: Vec<String> = (0..3).map(|i| format!("metric_{i}")).collect();
    let label_keys: Vec<String> = (0..3).map(|i| format!("key_{i}")).collect();
    let label_vals: Vec<String> = (0..3).map(|i| format!("val_{i}")).collect();

    let label_pairs: Vec<[LabelParam<'_>; 1]> = (0..3)
        .map(|i| {
            [LabelParam {
                key: &label_keys[i],
                value: &label_vals[i],
            }]
        })
        .collect();

    let params: Vec<MetricEntryParams<'_>> = (0..3)
        .map(|i| MetricEntryParams {
            metric_type: MetricType::Gauge,
            timestamp: 3000 + i as u64,
            name: &names[i],
            value: i as f64 * 10.0,
            source: Some(&sources[i]),
            service: Some("api"),
            labels: &label_pairs[i],
            temporality: Temporality::Unspecified,
            histogram: None,
            session_id: None,
        })
        .collect();

    let mut buf = Vec::new();
    let range = encode_metric_data_into(&mut buf, &params);
    let into_bytes = &buf[range];

    // Valid FlatBuffer root
    let root =
        u32::from_le_bytes([into_bytes[0], into_bytes[1], into_bytes[2], into_bytes[3]]) as usize;
    assert!(root < into_bytes.len());

    // All sources, names, labels present
    for i in 0..3 {
        let name = format!("metric_{i}");
        assert!(
            into_bytes.windows(name.len()).any(|w| w == name.as_bytes()),
            "name '{}' not found in encode_metric_data_into output",
            name,
        );
        let host = format!("host-{i}");
        assert!(
            into_bytes.windows(host.len()).any(|w| w == host.as_bytes()),
            "source '{}' not found in encode_metric_data_into output",
            host,
        );
        let lk = format!("key_{i}");
        assert!(
            into_bytes.windows(lk.len()).any(|w| w == lk.as_bytes()),
            "label key '{}' not found",
            lk,
        );
        let lv = format!("val_{i}");
        assert!(
            into_bytes.windows(lv.len()).any(|w| w == lv.as_bytes()),
            "label value '{}' not found",
            lv,
        );
    }
    assert!(into_bytes.windows(3).any(|w| w == b"api"));
}

#[test]
fn encode_metric_data_into_reuses_buffer() {
    let mut buf = Vec::new();

    let params = [MetricEntryParams {
        metric_type: MetricType::Gauge,
        timestamp: 100,
        name: "first",
        value: 1.0,
        source: None,
        service: None,
        labels: &[],
        temporality: Temporality::Unspecified,
        histogram: None,
        session_id: None,
    }];
    let range1 = encode_metric_data_into(&mut buf, &params);
    assert!(range1.end > range1.start);

    let params2 = [MetricEntryParams {
        metric_type: MetricType::Counter,
        timestamp: 200,
        name: "second",
        value: 2.0,
        source: None,
        service: None,
        labels: &[],
        temporality: Temporality::Delta,
        histogram: None,
        session_id: None,
    }];
    let range2 = encode_metric_data_into(&mut buf, &params2);
    assert_eq!(range2.start, range1.end);

    let into_bytes = &buf[range2.clone()];
    let root =
        u32::from_le_bytes([into_bytes[0], into_bytes[1], into_bytes[2], into_bytes[3]]) as usize;
    assert!(root < into_bytes.len());
}

#[test]
fn metric_type_values() {
    assert_eq!(MetricType::Unknown.as_u8(), 0);
    assert_eq!(MetricType::Gauge.as_u8(), 1);
    assert_eq!(MetricType::Counter.as_u8(), 2);
    assert_eq!(MetricType::Histogram.as_u8(), 3);
}

#[test]
fn temporality_values() {
    assert_eq!(Temporality::Unspecified.as_u8(), 0);
    assert_eq!(Temporality::Cumulative.as_u8(), 1);
    assert_eq!(Temporality::Delta.as_u8(), 2);
}

#[test]
fn encode_metric_entry_histogram() {
    let histogram = HistogramParams {
        count: 1000,
        sum: 45.5,
        min: 0.001,
        max: 2.3,
        buckets: vec![(0.01, 300), (0.1, 850), (1.0, 998), (f64::INFINITY, 1000)],
    };

    let bytes = encode_metric_entry(&MetricEntryParams {
        metric_type: MetricType::Histogram,
        timestamp: 1749587340000000000,
        name: "http.request.duration",
        value: 0.0,
        source: Some("server-1"),
        service: Some("api"),
        labels: &[LabelParam {
            key: "method",
            value: "GET",
        }],
        temporality: Temporality::Cumulative,
        histogram: Some(&histogram),
        session_id: None,
    });

    let root_offset = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
    assert!(root_offset < bytes.len());

    // metric_type = Histogram
    assert_eq!(bytes[root_offset + 48], MetricType::Histogram.as_u8());

    // name present
    assert!(
        bytes.windows(21).any(|w| w == b"http.request.duration"),
        "histogram name not found",
    );

    // source and service present
    assert!(
        bytes.windows(8).any(|w| w == b"server-1"),
        "source not found"
    );
    assert!(bytes.windows(3).any(|w| w == b"api"), "service not found");

    // label present
    assert!(
        bytes.windows(6).any(|w| w == b"method"),
        "label key not found"
    );
    assert!(
        bytes.windows(3).any(|w| w == b"GET"),
        "label value not found"
    );

    // Histogram data: count=1000, sum=45.5 should be in the binary as LE bytes
    let count_bytes = 1000u64.to_le_bytes();
    assert!(
        bytes.windows(8).any(|w| w == count_bytes),
        "histogram count not found",
    );
    let sum_bytes = 45.5f64.to_le_bytes();
    assert!(
        bytes.windows(8).any(|w| w == sum_bytes),
        "histogram sum not found",
    );
    let min_bytes = 0.001f64.to_le_bytes();
    assert!(
        bytes.windows(8).any(|w| w == min_bytes),
        "histogram min not found",
    );
    let max_bytes = 2.3f64.to_le_bytes();
    assert!(
        bytes.windows(8).any(|w| w == max_bytes),
        "histogram max not found",
    );

    // Bucket counts should appear
    let b300 = 300u64.to_le_bytes();
    assert!(
        bytes.windows(8).any(|w| w == b300),
        "bucket count 300 not found"
    );
    let b850 = 850u64.to_le_bytes();
    assert!(
        bytes.windows(8).any(|w| w == b850),
        "bucket count 850 not found"
    );
}

#[test]
fn encode_metric_entry_histogram_no_buckets() {
    let histogram = HistogramParams {
        count: 42,
        sum: 100.0,
        min: 1.0,
        max: 10.0,
        buckets: vec![],
    };

    let bytes = encode_metric_entry(&MetricEntryParams {
        metric_type: MetricType::Histogram,
        timestamp: 5000,
        name: "empty_buckets",
        value: 0.0,
        source: None,
        service: None,
        labels: &[],
        temporality: Temporality::Delta,
        histogram: Some(&histogram),
        session_id: None,
    });

    let root_offset = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
    assert!(root_offset < bytes.len());
    assert_eq!(bytes[root_offset + 48], MetricType::Histogram.as_u8());

    let count_bytes = 42u64.to_le_bytes();
    assert!(
        bytes.windows(8).any(|w| w == count_bytes),
        "histogram count not found"
    );
}

#[test]
fn encode_metric_data_mixed_types() {
    let histogram = HistogramParams {
        count: 500,
        sum: 25.0,
        min: 0.01,
        max: 1.5,
        buckets: vec![(0.1, 200), (1.0, 480), (f64::INFINITY, 500)],
    };

    let gauge = encode_metric_entry(&MetricEntryParams {
        metric_type: MetricType::Gauge,
        timestamp: 1000,
        name: "cpu.usage",
        value: 75.0,
        source: None,
        service: None,
        labels: &[],
        temporality: Temporality::Unspecified,
        histogram: None,
        session_id: None,
    });

    let counter = encode_metric_entry(&MetricEntryParams {
        metric_type: MetricType::Counter,
        timestamp: 1000,
        name: "requests.total",
        value: 42.0,
        source: None,
        service: None,
        labels: &[],
        temporality: Temporality::Delta,
        histogram: None,
        session_id: None,
    });

    let hist = encode_metric_entry(&MetricEntryParams {
        metric_type: MetricType::Histogram,
        timestamp: 1000,
        name: "latency",
        value: 0.0,
        source: None,
        service: None,
        labels: &[],
        temporality: Temporality::Cumulative,
        histogram: Some(&histogram),
        session_id: None,
    });

    let data = encode_metric_data(&[gauge, counter, hist]);
    let root = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
    assert!(root < data.len());

    // All three metric names present
    assert!(
        data.windows(9).any(|w| w == b"cpu.usage"),
        "gauge name not found"
    );
    assert!(
        data.windows(14).any(|w| w == b"requests.total"),
        "counter name not found"
    );
    assert!(
        data.windows(7).any(|w| w == b"latency"),
        "histogram name not found"
    );

    // Histogram count
    let count_bytes = 500u64.to_le_bytes();
    assert!(
        data.windows(8).any(|w| w == count_bytes),
        "histogram count not found in batch"
    );
}
