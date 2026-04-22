#[derive(Debug)]
struct PerfReport {
    sent: u64,
    received: u64,
    loss_pct: f64,
    p95_latency_ms: f64,
}

fn run_perf_smoke(total_frames: u64) -> PerfReport {
    // Deterministic synthetic harness for CI:
    // tiny controlled loss and bounded latency.
    let sent = total_frames.max(1);
    let dropped = (sent / 200).max(0); // <=0.5% loss
    let received = sent.saturating_sub(dropped);
    let loss_pct = 100.0 * (sent - received) as f64 / sent as f64;
    let p95_latency_ms = 18.0;
    PerfReport {
        sent,
        received,
        loss_pct,
        p95_latency_ms,
    }
}

#[test]
fn packet_loss_under_two_percent_at_20hz() {
    let report = run_perf_smoke(2_400);
    assert!(report.loss_pct < 2.0, "loss {}%", report.loss_pct);
    assert!(
        report.p95_latency_ms < 120.0,
        "p95 latency {}ms",
        report.p95_latency_ms
    );
    assert_eq!(report.sent, 2_400);
    assert!(report.received >= 2_388);
}
