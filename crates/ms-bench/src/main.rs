use serde::Deserialize;
use std::{env, fs, process::ExitCode};

const REGRESSION_LIMIT: f64 = 0.20;
const SC001_P50_LIMIT_MS: f64 = 150.0;
const SC001_P95_LIMIT_MS: f64 = 400.0;
const SC002_BINARY_LIMIT_BYTES: u64 = 30_000_000;
const SC002_IMAGE_LIMIT_BYTES: u64 = 40_000_000;
const SC003_IDLE_RSS_LIMIT_BYTES: u64 = 25_000_000;
const SC004_OVERHEAD_LIMIT_PERCENT: f64 = 10.0;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Report {
    schema_version: u32,
    runner: String,
    revision: String,
    metrics: Metrics,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Metrics {
    sc001_spawn_to_ready_ms: Distribution,
    sc002_agent_binary_bytes: u64,
    sc002_agent_image_bytes: u64,
    sc003_idle_rss_bytes: u64,
    sc004_five_member_overhead_percent: f64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Distribution {
    p50: f64,
    p95: f64,
}

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ms-bench: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Vec<String>) -> Result<(), String> {
    match args.as_slice() {
        [command, report] if command == "validate" => {
            let report = read_report(report)?;
            validate(&report)
        }
        [command, baseline, current] if command == "compare" => {
            let baseline = read_report(baseline)?;
            let current = read_report(current)?;
            compare(&baseline, &current)
        }
        _ => Err("usage: ms-bench validate <report.json> | ms-bench compare <baseline.json> <current.json>".into()),
    }
}

fn read_report(path: &str) -> Result<Report, String> {
    let contents = fs::read_to_string(path).map_err(|error| format!("{path}: {error}"))?;
    let report: Report = serde_json::from_str(&contents)
        .map_err(|error| format!("{path}: invalid benchmark report: {error}"))?;
    if report.schema_version != 1 {
        return Err(format!(
            "{path}: unsupported report schema version {}",
            report.schema_version
        ));
    }
    if report.runner.trim().is_empty() || report.revision.trim().is_empty() {
        return Err(format!("{path}: runner and revision must be non-empty"));
    }
    let durations = [
        report.metrics.sc001_spawn_to_ready_ms.p50,
        report.metrics.sc001_spawn_to_ready_ms.p95,
    ];
    if durations
        .iter()
        .any(|value| !value.is_finite() || *value < 0.0)
        || !report
            .metrics
            .sc004_five_member_overhead_percent
            .is_finite()
    {
        return Err(format!(
            "{path}: metrics must be finite, and duration values cannot be negative"
        ));
    }
    Ok(report)
}

fn validate(report: &Report) -> Result<(), String> {
    let metrics = &report.metrics;
    println!(
        "performance report for {} on {}",
        report.revision, report.runner
    );
    status(
        "SC-001 p50 spawn-to-agent.ready",
        metrics.sc001_spawn_to_ready_ms.p50,
        SC001_P50_LIMIT_MS,
        "ms",
    );
    status(
        "SC-001 p95 spawn-to-agent.ready",
        metrics.sc001_spawn_to_ready_ms.p95,
        SC001_P95_LIMIT_MS,
        "ms",
    );
    status_u64(
        "SC-002 static agent binary",
        metrics.sc002_agent_binary_bytes,
        SC002_BINARY_LIMIT_BYTES,
        "bytes",
    );
    status_u64(
        "SC-002 agent image",
        metrics.sc002_agent_image_bytes,
        SC002_IMAGE_LIMIT_BYTES,
        "bytes",
    );
    status_u64(
        "SC-003 idle RSS",
        metrics.sc003_idle_rss_bytes,
        SC003_IDLE_RSS_LIMIT_BYTES,
        "bytes",
    );
    status(
        "SC-004 lead plus four member overhead",
        metrics.sc004_five_member_overhead_percent,
        SC004_OVERHEAD_LIMIT_PERCENT,
        "%",
    );
    Ok(())
}

fn status(name: &str, actual: f64, maximum: f64, unit: &str) {
    let outcome = if actual <= maximum { "within" } else { "above" };
    println!("{name}: {actual:.2}{unit} ({outcome} target {maximum:.2}{unit})");
}

fn status_u64(name: &str, actual: u64, maximum: u64, unit: &str) {
    let outcome = if actual <= maximum { "within" } else { "above" };
    println!("{name}: {actual}{unit} ({outcome} target {maximum}{unit})");
}

fn compare(baseline: &Report, current: &Report) -> Result<(), String> {
    if baseline.runner != current.runner {
        return Err(format!(
            "runner mismatch: baseline {:?}, current {:?}",
            baseline.runner, current.runner
        ));
    }
    let mut failures = Vec::new();
    regression(
        &mut failures,
        "SC-001 p50 spawn-to-agent.ready",
        baseline.metrics.sc001_spawn_to_ready_ms.p50,
        current.metrics.sc001_spawn_to_ready_ms.p50,
    );
    regression(
        &mut failures,
        "SC-001 p95 spawn-to-agent.ready",
        baseline.metrics.sc001_spawn_to_ready_ms.p95,
        current.metrics.sc001_spawn_to_ready_ms.p95,
    );
    regression(
        &mut failures,
        "SC-002 static agent binary",
        baseline.metrics.sc002_agent_binary_bytes as f64,
        current.metrics.sc002_agent_binary_bytes as f64,
    );
    regression(
        &mut failures,
        "SC-002 agent image",
        baseline.metrics.sc002_agent_image_bytes as f64,
        current.metrics.sc002_agent_image_bytes as f64,
    );
    regression(
        &mut failures,
        "SC-003 idle RSS",
        baseline.metrics.sc003_idle_rss_bytes as f64,
        current.metrics.sc003_idle_rss_bytes as f64,
    );
    regression(
        &mut failures,
        "SC-004 lead plus four member overhead",
        baseline.metrics.sc004_five_member_overhead_percent,
        current.metrics.sc004_five_member_overhead_percent,
    );
    if failures.is_empty() {
        println!(
            "no metric regressed more than {:.0}% from {} to {} on {}",
            REGRESSION_LIMIT * 100.0,
            baseline.revision,
            current.revision,
            current.runner
        );
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

fn regression(failures: &mut Vec<String>, name: &str, baseline: f64, current: f64) {
    if current <= baseline {
        return;
    }
    if baseline == 0.0 {
        failures.push(format!(
            "{name} baseline is zero and current value is {current}"
        ));
        return;
    }
    let increase = (current - baseline) / baseline.abs();
    if increase > REGRESSION_LIMIT {
        failures.push(format!(
            "{name} regressed {:.1}%: baseline {baseline:.2}, current {current:.2} (limit >{:.0}%)",
            increase * 100.0,
            REGRESSION_LIMIT * 100.0
        ));
    }
}
