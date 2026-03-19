/// Benchmark scenario combining batch size and payload size.
#[derive(Debug, Clone, Copy)]
pub struct BenchScenario {
    pub name: &'static str,
    pub events_per_batch: usize,
    pub payload_size: usize,
}

impl BenchScenario {
    pub const fn total_bytes(&self) -> usize {
        self.events_per_batch * self.payload_size
    }
}

/// Standard benchmark scenarios.
pub const SCENARIOS: &[BenchScenario] = &[
    BenchScenario {
        name: "realtime_small",
        events_per_batch: 10,
        payload_size: 100,
    },
    BenchScenario {
        name: "typical",
        events_per_batch: 100,
        payload_size: 200,
    },
    BenchScenario {
        name: "high_volume",
        events_per_batch: 500,
        payload_size: 200,
    },
    BenchScenario {
        name: "large_events",
        events_per_batch: 100,
        payload_size: 1000,
    },
];

/// Quick scenarios for fast iteration.
pub const SCENARIOS_QUICK: &[BenchScenario] = &[SCENARIOS[1], SCENARIOS[2]];

/// Generate a JSON payload of approximately the given size in bytes.
pub fn generate_payload(size: usize) -> Vec<u8> {
    // Build a JSON object with enough key-value pairs to reach target size.
    // Each pair is ~20 bytes: "kNN":"value_padding..."
    let mut obj = serde_json::Map::new();
    obj.insert(
        "user_id".to_string(),
        serde_json::Value::String("user_bench_123".to_string()),
    );
    obj.insert(
        "event".to_string(),
        serde_json::Value::String("Benchmark Event".to_string()),
    );

    let base = serde_json::to_vec(&obj).unwrap();
    if base.len() >= size {
        return base;
    }

    // Pad with a single large string field to reach target size
    let remaining = size.saturating_sub(base.len() + 20); // account for key overhead
    let padding = "x".repeat(remaining);
    obj.insert("data".to_string(), serde_json::Value::String(padding));

    serde_json::to_vec(&obj).unwrap()
}
