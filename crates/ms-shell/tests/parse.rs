use ms_shell::parse_shell_command;

#[test]
fn simple_shell_command_reports_program_and_arguments() {
    let parsed = parse_shell_command("cargo test --workspace").unwrap();
    assert_eq!(parsed.kind, "simple");
    assert_eq!(parsed.commands, ["cargo test --workspace"]);
}
