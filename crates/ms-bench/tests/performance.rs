#[path = "../../../tests/integration/harness.rs"]
mod harness;

use harness::IntegrationHarness;
use rusqlite::Connection;
use serde_json::json;
use std::{
    env, fs,
    io::{BufRead, Write},
    os::unix::net::UnixStream,
    path::Path,
    process::Command,
    thread,
    time::{Duration, Instant},
};

#[test]
fn collect_phase9_performance_report() {
    let samples = env::var("MS_BENCH_SAMPLES")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(10)
        .max(3);
    let swarm_samples = env::var("MS_BENCH_SWARM_SAMPLES")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(3)
        .max(1);
    let agent_image =
        env::var("MS_BENCH_AGENT_IMAGE").unwrap_or_else(|_| "microswarm-agent:dev".into());
    let agent_binary = env::var("MS_BENCH_AGENT_BINARY")
        .unwrap_or_else(|_| "target/x86_64-unknown-linux-musl/release/microswarm-agent".into());
    let binary_bytes = fs::metadata(&agent_binary)
        .unwrap_or_else(|error| panic!("cannot inspect static agent {agent_binary}: {error}"))
        .len();
    let image_bytes = docker_image_bytes(&agent_image);

    let mut ready_ms = Vec::with_capacity(samples);
    let mut idle_rss_bytes = Vec::with_capacity(samples);
    for _ in 0..samples {
        let Some(integration) = IntegrationHarness::start_or_skip() else {
            return;
        };
        let started = Instant::now();
        let response = rpc(
            &integration.home,
            json!({"op":"chat","session_id":null,"text":"measure agent readiness"}),
        );
        let requested_session_id = response["session_id"]
            .as_str()
            .expect("chat response contains session_id")
            .to_owned();
        let session_id = wait_for_agent_ready(&integration.home, Duration::from_secs(30));
        assert_eq!(session_id, requested_session_id);
        ready_ms.push(started.elapsed().as_secs_f64() * 1_000.0);
        if response["pending"].as_bool() == Some(true) {
            let after_id = response["after_id"]
                .as_i64()
                .expect("pending response contains after_id");
            wait_for_reply(&integration.home, &session_id, after_id);
        }
        let containers = integration
            .containers_for(&session_id)
            .expect("list session containers");
        assert_eq!(
            containers.len(),
            1,
            "single-agent benchmark needs one container"
        );
        idle_rss_bytes.push(docker_container_rss(&containers[0]));
        drop(integration);
    }

    let mut overhead_percent = Vec::with_capacity(swarm_samples);
    for _ in 0..swarm_samples {
        let Some(single) = IntegrationHarness::start_or_skip() else {
            return;
        };
        let (single_session, _) = chat_turn(&single.home, None, "warm-single-container");
        let single_model_before = single.model.elapsed_model_time();
        let single_started = Instant::now();
        let (_, single_reply) =
            chat_turn(&single.home, Some(&single_session), "single-benchmark-turn");
        let single_wall = single_started.elapsed();
        let single_model = single
            .model
            .elapsed_model_time()
            .saturating_sub(single_model_before);
        let single_adjusted = single_wall.saturating_sub(single_model).as_secs_f64();
        assert_eq!(single_reply, "single turn complete");
        drop(single);

        let Some(swarm) = IntegrationHarness::start_benchmark_swarm_or_skip() else {
            return;
        };
        let (swarm_session, _) = chat_turn(&swarm.home, None, "benchmark-prepare-four-members");
        let session_count = count_sessions(&swarm.home);
        assert_eq!(
            session_count, 5,
            "SC-004 requires one lead and four members"
        );
        wait_for_initial_member_replies(&swarm.home, Duration::from_secs(30));
        let swarm_model_before = swarm.model.elapsed_model_time();
        let swarm_started = Instant::now();
        let (_, swarm_reply) =
            chat_turn(&swarm.home, Some(&swarm_session), "benchmark-four-members");
        let swarm_wall = swarm_started.elapsed();
        let swarm_model = swarm
            .model
            .elapsed_model_time()
            .saturating_sub(swarm_model_before);
        let swarm_adjusted = swarm_wall.saturating_sub(swarm_model).as_secs_f64();
        assert_eq!(swarm_reply, "four member benchmark complete");
        if single_adjusted <= 0.0 {
            panic!("single-agent non-model time was zero; cannot calculate SC-004");
        }
        let overhead = ((swarm_adjusted / single_adjusted) - 1.0) * 100.0;
        println!(
            "SC-004 sample: single {:.2}ms (model {:.2}ms), lead plus four {:.2}ms (model {:.2}ms), overhead {:.1}%",
            single_wall.as_secs_f64() * 1_000.0,
            single_model.as_secs_f64() * 1_000.0,
            swarm_wall.as_secs_f64() * 1_000.0,
            swarm_model.as_secs_f64() * 1_000.0,
            overhead
        );
        overhead_percent.push(overhead);
        drop(swarm);
    }

    let revision = env::var("MS_BENCH_REVISION")
        .or_else(|_| env::var("GITHUB_SHA"))
        .unwrap_or_else(|_| git_revision());
    let runner = env::var("MS_BENCH_RUNNER").unwrap_or_else(|_| "local-unpinned".into());
    let report = json!({
        "schema_version": 1,
        "runner": runner,
        "revision": revision,
        "metrics": {
            "sc001_spawn_to_ready_ms": {
                "p50": percentile(&ready_ms, 0.50),
                "p95": percentile(&ready_ms, 0.95)
            },
            "sc002_agent_binary_bytes": binary_bytes,
            "sc002_agent_image_bytes": image_bytes,
            "sc003_idle_rss_bytes": percentile_u64(&idle_rss_bytes, 0.50),
            "sc004_five_member_overhead_percent": percentile(&overhead_percent, 0.50)
        }
    });
    let output = env::var("MS_BENCH_OUTPUT").unwrap_or_else(|_| "benchmark-report.json".into());
    fs::write(&output, serde_json::to_vec_pretty(&report).unwrap())
        .unwrap_or_else(|error| panic!("cannot write benchmark report {output}: {error}"));
    println!("MICROSWARM_BENCH_REPORT={output}");
}

fn chat_turn(home: &Path, session_id: Option<&str>, prompt: &str) -> (String, String) {
    let mut response = rpc(
        home,
        json!({"op":"chat","session_id":session_id,"text":prompt}),
    );
    let session_id = response["session_id"]
        .as_str()
        .expect("chat response contains session_id")
        .to_owned();
    if response["pending"].as_bool() == Some(true) {
        let after_id = response["after_id"]
            .as_i64()
            .expect("pending response contains after_id");
        response = wait_for_reply(home, &session_id, after_id);
    }
    if let Some(error) = response["error"].as_str() {
        panic!("supervisor chat RPC failed: {error}");
    }
    let text = response["text"].as_str().unwrap_or_default().to_owned();
    (session_id, text)
}

fn wait_for_reply(home: &Path, session_id: &str, after_id: i64) -> serde_json::Value {
    let response = rpc(
        home,
        json!({"op":"wait","session_id":session_id,"after_id":after_id}),
    );
    if let Some(error) = response["error"].as_str() {
        panic!("supervisor wait RPC failed: {error}");
    }
    response
}

fn rpc(home: &Path, request: serde_json::Value) -> serde_json::Value {
    let stream = UnixStream::connect(home.join("supervisor.sock")).expect("connect supervisor");
    let mut writer = stream.try_clone().expect("clone supervisor socket");
    writeln!(writer, "{request}").expect("send supervisor request");
    let mut response = String::new();
    std::io::BufReader::new(stream)
        .read_line(&mut response)
        .expect("read supervisor response");
    serde_json::from_str(&response).expect("supervisor response is JSON")
}

fn wait_for_agent_ready(home: &Path, timeout: Duration) -> String {
    let deadline = Instant::now() + timeout;
    loop {
        for session_dir in fs::read_dir(home.join("sessions"))
            .expect("read sessions directory")
            .filter_map(Result::ok)
            .map(|entry| entry.path())
        {
            let events_path = session_dir.join("out/events.db");
            if !events_path.is_file() {
                continue;
            }
            let Ok(database) = Connection::open(&events_path) else {
                continue;
            };
            let ready = database
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM events WHERE type='agent.ready')",
                    [],
                    |row| row.get::<_, bool>(0),
                )
                .unwrap_or(false);
            if ready {
                return session_dir
                    .file_name()
                    .expect("session directory has a name")
                    .to_string_lossy()
                    .into_owned();
            }
        }
        assert!(
            Instant::now() < deadline,
            "agent.ready was not recorded in time"
        );
        thread::sleep(Duration::from_millis(2));
    }
}

fn docker_container_rss(container_id: &str) -> u64 {
    let output = Command::new("docker")
        .args(["inspect", "--format", "{{.State.Pid}}", container_id])
        .output()
        .expect("inspect agent container process");
    assert!(
        output.status.success(),
        "docker inspect failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let pid = String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<u32>()
        .expect("Docker reports the host PID for the agent container");
    let status = fs::read_to_string(format!("/proc/{pid}/status"))
        .unwrap_or_else(|error| panic!("cannot read container process RSS: {error}"));
    let resident_kib = status
        .lines()
        .find_map(|line| line.strip_prefix("VmRSS:")?.split_whitespace().next())
        .and_then(|value| value.parse::<u64>().ok())
        .expect("/proc status includes VmRSS");
    resident_kib * 1_024
}

fn docker_image_bytes(image: &str) -> u64 {
    let output = Command::new("docker")
        .args(["image", "inspect", "--format", "{{.Size}}", image])
        .output()
        .expect("inspect agent image");
    assert!(
        output.status.success(),
        "docker image inspect failed for {image}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .expect("Docker image size is an integer")
}

fn count_sessions(home: &Path) -> usize {
    let database = Connection::open(home.join("supervisor.db")).expect("open supervisor DB");
    database
        .query_row("SELECT COUNT(*) FROM sessions", [], |row| {
            row.get::<_, i64>(0)
        })
        .expect("count sessions") as usize
}

fn wait_for_initial_member_replies(home: &Path, timeout: Duration) {
    let database = Connection::open(home.join("supervisor.db")).expect("open supervisor DB");
    let mut statement = database
        .prepare("SELECT id FROM sessions WHERE kind='member'")
        .expect("query member ids");
    let members = statement
        .query_map([], |row| row.get::<_, String>(0))
        .expect("read member ids")
        .collect::<Result<Vec<_>, _>>()
        .expect("collect member ids");
    let deadline = Instant::now() + timeout;
    loop {
        let ready = members.iter().all(|member| {
            let Ok(outbound) =
                Connection::open(home.join("sessions").join(member).join("out/outbound.db"))
            else {
                return false;
            };
            outbound
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM outbound WHERE kind='reply')",
                    [],
                    |row| row.get::<_, bool>(0),
                )
                .unwrap_or(false)
        });
        if ready {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "all four members must complete their setup task before SC-004"
        );
        thread::sleep(Duration::from_millis(5));
    }
}

fn percentile(values: &[f64], fraction: f64) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let index = ((sorted.len() as f64 * fraction).ceil() as usize)
        .saturating_sub(1)
        .min(sorted.len() - 1);
    sorted[index]
}

fn percentile_u64(values: &[u64], fraction: f64) -> u64 {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let index = ((sorted.len() as f64 * fraction).ceil() as usize)
        .saturating_sub(1)
        .min(sorted.len() - 1);
    sorted[index]
}

fn git_revision() -> String {
    Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .unwrap_or_else(|| "unknown".into())
}
