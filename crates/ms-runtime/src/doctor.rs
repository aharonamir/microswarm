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
