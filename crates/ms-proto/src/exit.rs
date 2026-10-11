use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExitReason {
    Ok,
    Transient,
    Safety,
    Config,
}

pub const fn exit_code(reason: ExitReason) -> i32 {
    match reason {
        ExitReason::Ok => 0,
        ExitReason::Transient => 75,
        ExitReason::Safety => 77,
        ExitReason::Config => 78,
    }
}
