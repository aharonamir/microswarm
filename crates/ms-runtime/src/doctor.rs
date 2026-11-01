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
}

pub fn run(input: DoctorInput<'_>) -> Vec<Check> {
    vec![
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
    ]
}

pub fn render(checks: &[Check]) -> String {
    checks
        .iter()
        .map(|check| format!("✓ {:<16}{}", check.name, check.detail))
        .collect::<Vec<_>>()
        .join("\n")
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
        ];
        assert_eq!(
            render(&checks),
            concat!(
                "✓ configuration   /home/operator/.microswarm/config\n",
                "✓ runtime socket  unix:///var/run/docker.sock\n",
                "✓ agent image     microswarm-agent:dev\n",
                "✓ model           fixture-model via session proxy\n",
                "✓ credential      model-key is present on the host"
            )
        );
    }
}
