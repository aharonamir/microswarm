use ms_proto::ExitReason;

pub fn reason_for_code(code: i32) -> ExitReason {
    match code {
        0 => ExitReason::Ok,
        75 => ExitReason::Transient,
        77 => ExitReason::Safety,
        78 => ExitReason::Config,
        _ => ExitReason::Safety,
    }
}

pub fn reason_name(reason: ExitReason) -> &'static str {
    match reason {
        ExitReason::Ok => "ok",
        ExitReason::Transient => "transient",
        ExitReason::Safety => "safety",
        ExitReason::Config => "config",
    }
}
