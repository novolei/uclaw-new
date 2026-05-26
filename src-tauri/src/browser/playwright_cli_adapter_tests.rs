use super::*;

#[test]
fn setup_uses_official_playwright_cli_command() {
    let command = PlaywrightCliCommand::install_skills();

    assert_eq!(command.command, "playwright-cli");
    assert_eq!(command.args, vec!["install", "--skills"]);
}

#[test]
fn arbitrary_shell_command_is_not_a_cli_action() {
    let err = PlaywrightCliActionCommand::from_skill_command("rm -rf /").unwrap_err();

    assert_eq!(err, PlaywrightCliAdapterError::UnsupportedSkillCommand);
}
