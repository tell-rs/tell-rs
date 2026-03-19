use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};

use tell_bench::{SCENARIOS, generate_payload};
use tell_encoding::{
    BatchParams, EventParams, EventType, LogEntryParams, LogEventType, LogLevel, SchemaType,
    encode_batch, encode_event, encode_event_data, encode_event_data_into, encode_log_data,
    encode_log_data_into, encode_log_entry,
};

fn bench_encode_event(c: &mut Criterion) {
    let mut group = c.benchmark_group("encode_event");

    let device_id = [0x42u8; 16];
    let session_id = [0x43u8; 16];

    for scenario in SCENARIOS {
        let payload = generate_payload(scenario.payload_size);

        group.throughput(Throughput::Bytes(payload.len() as u64));
        group.bench_with_input(
            BenchmarkId::new("event", scenario.name),
            &payload,
            |b, payload| {
                b.iter(|| {
                    encode_event(&EventParams {
                        event_type: EventType::Track,
                        timestamp: 1700000000000,
                        service: None,
                        device_id: Some(&device_id),
                        session_id: Some(&session_id),
                        event_name: Some("Page Viewed"),
                        payload: Some(payload),
                    })
                });
            },
        );
    }

    group.finish();
}

fn bench_encode_event_data(c: &mut Criterion) {
    let mut group = c.benchmark_group("encode_event_data");

    let device_id = [0x42u8; 16];
    let session_id = [0x43u8; 16];
    let payload = generate_payload(200);

    for &batch_size in &[10usize, 100, 500] {
        // Pre-encode events
        let encoded: Vec<Vec<u8>> = (0..batch_size)
            .map(|_| {
                encode_event(&EventParams {
                    event_type: EventType::Track,
                    timestamp: 1700000000000,
                    service: None,
                    device_id: Some(&device_id),
                    session_id: Some(&session_id),
                    event_name: Some("Page Viewed"),
                    payload: Some(&payload),
                })
            })
            .collect();

        let total_bytes: usize = encoded.iter().map(|e| e.len()).sum();
        group.throughput(Throughput::Bytes(total_bytes as u64));
        group.bench_with_input(
            BenchmarkId::new("batch", batch_size),
            &encoded,
            |b, encoded| {
                b.iter(|| encode_event_data(encoded));
            },
        );
    }

    group.finish();
}

fn bench_encode_full_batch(c: &mut Criterion) {
    let mut group = c.benchmark_group("encode_full_batch");

    let api_key = [0xA1u8; 16];
    let device_id = [0x42u8; 16];
    let session_id = [0x43u8; 16];

    for scenario in SCENARIOS {
        let payload = generate_payload(scenario.payload_size);

        group.throughput(Throughput::Elements(scenario.events_per_batch as u64));
        group.bench_with_input(
            BenchmarkId::new("events", scenario.name),
            &payload,
            |b, payload| {
                b.iter(|| {
                    let encoded: Vec<Vec<u8>> = (0..scenario.events_per_batch)
                        .map(|_| {
                            encode_event(&EventParams {
                                event_type: EventType::Track,
                                timestamp: 1700000000000,
                                service: None,
                                device_id: Some(&device_id),
                                session_id: Some(&session_id),
                                event_name: Some("Page Viewed"),
                                payload: Some(payload),
                            })
                        })
                        .collect();

                    let event_data = encode_event_data(&encoded);

                    encode_batch(&BatchParams {
                        api_key: &api_key,
                        schema_type: SchemaType::Event,
                        version: 100,
                        batch_id: 1,
                        data: &event_data,
                    })
                });
            },
        );
    }

    group.finish();
}

fn bench_encode_log_entry(c: &mut Criterion) {
    let mut group = c.benchmark_group("encode_log_entry");

    let session_id = [0x43u8; 16];

    for scenario in SCENARIOS {
        let payload = generate_payload(scenario.payload_size);

        group.throughput(Throughput::Bytes(payload.len() as u64));
        group.bench_with_input(
            BenchmarkId::new("log", scenario.name),
            &payload,
            |b, payload| {
                b.iter(|| {
                    encode_log_entry(&LogEntryParams {
                        event_type: LogEventType::Log,
                        session_id: Some(&session_id),
                        level: LogLevel::Error,
                        timestamp: 1700000000000,
                        source: Some("bench-host"),
                        service: Some("api"),
                        payload: Some(payload),
                    })
                });
            },
        );
    }

    group.finish();
}

fn bench_encode_log_batch(c: &mut Criterion) {
    let mut group = c.benchmark_group("encode_log_batch");

    let api_key = [0xA1u8; 16];
    let session_id = [0x43u8; 16];
    let payload = generate_payload(200);

    for &batch_size in &[10usize, 100, 500] {
        let encoded: Vec<Vec<u8>> = (0..batch_size)
            .map(|_| {
                encode_log_entry(&LogEntryParams {
                    event_type: LogEventType::Log,
                    session_id: Some(&session_id),
                    level: LogLevel::Info,
                    timestamp: 1700000000000,
                    source: Some("bench-host"),
                    service: Some("api"),
                    payload: Some(&payload),
                })
            })
            .collect();

        group.throughput(Throughput::Elements(batch_size as u64));
        group.bench_with_input(
            BenchmarkId::new("logs", batch_size),
            &encoded,
            |b, encoded| {
                b.iter(|| {
                    let log_data = encode_log_data(encoded);
                    encode_batch(&BatchParams {
                        api_key: &api_key,
                        schema_type: SchemaType::Log,
                        version: 100,
                        batch_id: 1,
                        data: &log_data,
                    })
                });
            },
        );
    }

    group.finish();
}

fn bench_encode_full_batch_into(c: &mut Criterion) {
    let mut group = c.benchmark_group("encode_full_batch_into");

    let api_key = [0xA1u8; 16];
    let device_id = [0x42u8; 16];
    let session_id = [0x43u8; 16];

    for scenario in SCENARIOS {
        let payload = generate_payload(scenario.payload_size);

        // Build params once (borrowed from locals)
        let params: Vec<EventParams<'_>> = (0..scenario.events_per_batch)
            .map(|_| EventParams {
                event_type: EventType::Track,
                timestamp: 1700000000000,
                service: None,
                device_id: Some(&device_id),
                session_id: Some(&session_id),
                event_name: Some("Page Viewed"),
                payload: Some(&payload),
            })
            .collect();

        group.throughput(Throughput::Elements(scenario.events_per_batch as u64));
        group.bench_with_input(
            BenchmarkId::new("events", scenario.name),
            &params,
            |b, params| {
                let mut buf = Vec::with_capacity(64 * 1024);
                b.iter(|| {
                    buf.clear();
                    let range = encode_event_data_into(&mut buf, params);
                    encode_batch(&BatchParams {
                        api_key: &api_key,
                        schema_type: SchemaType::Event,
                        version: 100,
                        batch_id: 1,
                        data: &buf[range],
                    })
                });
            },
        );
    }

    group.finish();
}

fn bench_encode_log_batch_into(c: &mut Criterion) {
    let mut group = c.benchmark_group("encode_log_batch_into");

    let api_key = [0xA1u8; 16];
    let session_id = [0x43u8; 16];
    let payload = generate_payload(200);

    for &batch_size in &[10usize, 100, 500] {
        let params: Vec<LogEntryParams<'_>> = (0..batch_size)
            .map(|_| LogEntryParams {
                event_type: LogEventType::Log,
                session_id: Some(&session_id),
                level: LogLevel::Info,
                timestamp: 1700000000000,
                source: Some("bench-host"),
                service: Some("api"),
                payload: Some(&payload),
            })
            .collect();

        group.throughput(Throughput::Elements(batch_size as u64));
        group.bench_with_input(
            BenchmarkId::new("logs", batch_size),
            &params,
            |b, params| {
                let mut buf = Vec::with_capacity(64 * 1024);
                b.iter(|| {
                    buf.clear();
                    let range = encode_log_data_into(&mut buf, params);
                    encode_batch(&BatchParams {
                        api_key: &api_key,
                        schema_type: SchemaType::Log,
                        version: 100,
                        batch_id: 1,
                        data: &buf[range],
                    })
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_encode_event,
    bench_encode_event_data,
    bench_encode_full_batch,
    bench_encode_full_batch_into,
    bench_encode_log_entry,
    bench_encode_log_batch,
    bench_encode_log_batch_into,
);
criterion_main!(benches);
