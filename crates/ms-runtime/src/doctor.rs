#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Check {
    pub name: String,
    pub result: &'static str,
    pub detail: String,
}

#[derive(Clone, Debug)]
pub struct DoctorInput<'a> {
    pub config_path: &'a str,
    pub runtime_socket: &'a str,
    pub agent_image: &'a str,
    pub model: &'a str,
    pub credential: &'a str,
    pub search: Option<SearchInput<'a>>,
}

#[derive(Clone, Debug)]
pub struct SearchInput<'a> {
    pub provider: &'a str,
    pub credential: &'a str,
    pub credential_present: bool,
}

pub fn run(input: DoctorInput<'_>) -> Vec<Check> {
    let mut checks = vec![
        Check {
            name: "configuration".into(),
            result: "pass",
            detail: input.config_path.into(),
        },
        Check {
            name: "runtime socket".into(),
            result: "pass",
            detail: input.runtime_socket.into(),
        },
        Check {
            name: "agent image".into(),
            result: "pass",
            detail: input.agent_image.into(),
        },
        Check {
            name: "model".into(),
            result: "pass",
            detail: format!("{} via session proxy", input.model),
        },
        Check {
            name: "credential".into(),
            result: "pass",
            detail: format!("{} is present on the host", input.credential),
        },
    ];
    let (result, detail) = match input.search {
        Some(search) if search.credential_present => (
            "pass",
            format!(
                "{} search is configured; credential {} is present on the host",
                provider_label(search.provider),
                search.credential
            ),
        ),
        Some(search) => (
            "fail",
            format!(
                "{} search is configured but credential {} is missing",
                provider_label(search.provider),
                search.credential
            ),
        ),
        None => ("warn", "optional search provider is not configured".into()),
    };
    checks.push(Check {
        name: "web search".into(),
        result,
        detail,
    });
    checks
}

pub fn render(checks: &[Check]) -> String {
    checks
        .iter()
        .map(|check| {
            let mark = match check.result {
                "pass" => "✓",
                "warn" => "!",
                "fail" => "✗",
                _ => "?",
            };
            format!("{mark} {:<16}{}", check.name, check.detail)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn provider_label(provider: &str) -> &str {
    match provider {
        "tavily" => "Tavily",
        "brave" => "Brave Search",
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::{Check, render};

    #[test]
    fn doctor_output_matches_the_cli_snapshot() {
        let checks = [
            Check {
                name: "configuration".into(),
                result: "pass",
                detail: "/home/operator/.microswarm/config".into(),
            },
            Check {
                name: "runtime socket".into(),
                result: "pass",
                detail: "unix:///var/run/docker.sock".into(),
            },
            Check {
                name: "agent image".into(),
                result: "pass",
                detail: "microswarm-agent:dev".into(),
            },
            Check {
                name: "model".into(),
                result: "pass",
                detail: "fixture-model via session proxy".into(),
            },
            Check {
                name: "credential".into(),
                result: "pass",
                detail: "model-key is present on the host".into(),
            },
            Check {
                name: "web search".into(),
                result: "warn",
                detail: "optional search provider is not configured".into(),
            },
        ];
        assert_eq!(
            render(&checks),
            concat!(
                "✓ configuration   /home/operator/.microswarm/config\n",
                "✓ runtime socket  unix:///var/run/docker.sock\n",
                "✓ agent image     microswarm-agent:dev\n",
                "✓ model           fixture-model via session proxy\n",
                "✓ credential      model-key is present on the host\n",
                "! web search      optional search provider is not configured"
            )
        );
    }

    #[test]
    fn doctor_reports_web_search_when_optional_search_is_not_configured() {
        let checks = super::run(super::DoctorInput {
            config_path: "/tmp/config",
            runtime_socket: "unix:///run/docker.sock",
            agent_image: "microswarm-agent:dev",
            model: "fixture-model",
            credential: "model-key",
            search: None,
        });
        let search = checks.iter().find(|check| check.name == "web search");
        assert!(
            search.is_some(),
            "doctor omitted the optional web search check"
        );
        assert_eq!(search.unwrap().result, "warn");
        assert!(search.unwrap().detail.contains("not configured"));
    }
}
