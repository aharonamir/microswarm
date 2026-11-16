use ms_console::status::{SessionStatus, status_words};

#[test]
fn status_words_cover_data_model_conditions() {
    let cases = [
        (SessionStatus::pending(), "Waiting for you", "attention"),
        (SessionStatus::starting(), "Starting", "neutral"),
        (SessionStatus::busy(), "Working", "active"),
        (
            SessionStatus::waiting_team(),
            "Waiting on the team",
            "active",
        ),
        (SessionStatus::crashed(), "Restarting after a crash", "warn"),
        (SessionStatus::idle(8), "Idle for 8 min", "neutral"),
        (SessionStatus::ready(), "Ready for your next message", "ok"),
        (
            SessionStatus::ended_by_operator(),
            "Ended by you",
            "neutral",
        ),
        (SessionStatus::closed("idle"), "Ended · idle", "neutral"),
        (SessionStatus::closed("dismissed"), "Finished", "neutral"),
        (
            SessionStatus::failed("safety"),
            "Stopped for safety",
            "danger",
        ),
        (
            SessionStatus::failed("timeout"),
            "Stopped · timeout",
            "danger",
        ),
    ];
    for (input, words, tone) in cases {
        let actual = status_words(&input);
        assert_eq!(actual.en, words);
        assert_eq!(actual.tone, tone);
    }
}
