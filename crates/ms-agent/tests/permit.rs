use ms_agent::permit::{PermitAction, evaluate_shell};

#[test]
fn parseable_complex_shell_asks_with_the_parsed_command_structure() {
    let command = "printf '%s\\n' ready | cat";
    let decision = evaluate_shell(command);

    assert_eq!(decision.action, PermitAction::Ask);
    let prompt = decision
        .approval_prompt
        .expect("complex shell asks for approval");
    assert_eq!(prompt.raw_command, command);
    assert_eq!(prompt.parsed_structure.kind, "pipeline");
    assert_eq!(
        prompt.parsed_structure.commands,
        ["printf '%s\\n' ready", "cat"]
    );
}

#[test]
fn unparseable_shell_is_denied_with_structured_rewrite_guidance() {
    let decision = evaluate_shell("printf '%s\\n' 'unfinished");

    assert_eq!(decision.action, PermitAction::Deny);
    assert!(decision.approval_prompt.is_none());
    let error = decision
        .tool_error
        .expect("deny result has a structured tool error");
    assert_eq!(error.code, "shell_parse_failed");
    assert!(error.message.contains("could not be parsed"));
    assert!(error.rewrite_guidance.contains("simpler"));
}
