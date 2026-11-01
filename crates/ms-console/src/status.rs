#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatusWords {
    pub en: String,
    pub zh: String,
    pub tone: String,
}

#[derive(Clone, Debug)]
pub struct SessionStatus {
    pub status: String,
    pub close_reason: Option<String>,
    pub pending_approval: bool,
    pub waiting_team: bool,
    pub idle_minutes: Option<u64>,
    pub ended_by_operator: bool,
    pub failure_reason: Option<String>,
}

impl SessionStatus {
    pub fn pending() -> Self {
        Self::base("ready").with_pending()
    }
    pub fn starting() -> Self {
        Self::base("starting")
    }
    pub fn busy() -> Self {
        Self::base("busy")
    }
    pub fn waiting_team() -> Self {
        Self::base("ready").with_waiting_team()
    }
    pub fn crashed() -> Self {
        Self::base("crashed")
    }
    pub fn idle(minutes: u64) -> Self {
        Self {
            idle_minutes: Some(minutes),
            ..Self::base("ready")
        }
    }
    pub fn ready() -> Self {
        Self::base("ready")
    }
    pub fn ended_by_operator() -> Self {
        Self {
            ended_by_operator: true,
            ..Self::base("closed")
        }
    }
    pub fn closed(reason: &str) -> Self {
        Self {
            close_reason: Some(reason.into()),
            ..Self::base("closed")
        }
    }
    pub fn failed(reason: &str) -> Self {
        Self {
            failure_reason: Some(reason.into()),
            ..Self::base("failed")
        }
    }
    fn base(status: &str) -> Self {
        Self {
            status: status.into(),
            close_reason: None,
            pending_approval: false,
            waiting_team: false,
            idle_minutes: None,
            ended_by_operator: false,
            failure_reason: None,
        }
    }
    fn with_pending(mut self) -> Self {
        self.pending_approval = true;
        self
    }
    fn with_waiting_team(mut self) -> Self {
        self.waiting_team = true;
        self
    }
}

pub fn status_words(input: &SessionStatus) -> StatusWords {
    let (en, zh, tone) = if input.pending_approval {
        ("Waiting for you".into(), "等待你的确认".into(), "attention")
    } else {
        match input.status.as_str() {
            "starting" => ("Starting".into(), "正在启动".into(), "neutral"),
            "busy" => ("Working".into(), "正在处理".into(), "active"),
            "ready" if input.waiting_team => (
                "Waiting on the team".into(),
                "等待团队成员".into(),
                "active",
            ),
            "crashed" => (
                "Restarting after a crash".into(),
                "崩溃后正在重启".into(),
                "warn",
            ),
            "ready" if input.idle_minutes.is_some() => (
                format!("Idle for {} min", input.idle_minutes.unwrap()),
                format!("已空闲 {} 分钟", input.idle_minutes.unwrap()),
                "neutral",
            ),
            "ready" => (
                "Ready for your next message".into(),
                "可以继续发送消息".into(),
                "ok",
            ),
            "closed" if input.ended_by_operator => {
                ("Ended by you".into(), "已由你结束".into(), "neutral")
            }
            "closed" if input.close_reason.as_deref() == Some("session_end") => {
                ("Ended".into(), "已结束".into(), "neutral")
            }
            "closed" if input.close_reason.as_deref() == Some("idle") => {
                ("Ended · idle".into(), "已结束 · 空闲超时".into(), "neutral")
            }
            "closed" if input.close_reason.as_deref() == Some("dismissed") => {
                ("Finished".into(), "已完成".into(), "neutral")
            }
            "closed" => ("Ended".into(), "已结束".into(), "neutral"),
            "failed" if input.failure_reason.as_deref() == Some("safety") => (
                "Stopped for safety".into(),
                "已出于安全原因停止".into(),
                "danger",
            ),
            "failed" => {
                let reason = input.failure_reason.as_deref().unwrap_or("unknown");
                (
                    format!("Stopped · {reason}"),
                    format!("已停止 · {reason}"),
                    "danger",
                )
            }
            _ => ("Unknown".into(), "未知".into(), "neutral"),
        }
    };
    StatusWords {
        en,
        zh,
        tone: tone.into(),
    }
}
