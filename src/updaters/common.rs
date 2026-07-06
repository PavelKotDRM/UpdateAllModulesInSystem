use crate::model::PackageUpdate;
use crate::updater::{capture_command, heuristic_parse_updates, stream_command, CommandOutput, UpdaterError};
use std::sync::mpsc::Sender;

fn command_failed(program: &str, output: CommandOutput) -> UpdaterError {
    UpdaterError::CommandFailed {
        program: program.to_owned(),
        code: output.exit_code,
        stderr: output.stderr,
    }
}

pub(super) fn stream_checked(
    program: &str,
    args: &[String],
    log_sender: &Sender<String>,
) -> Result<(), UpdaterError> {
    let output = stream_command(program, args, log_sender)?;
    if output.success {
        Ok(())
    } else {
        Err(command_failed(program, output))
    }
}

pub(super) fn stream_checked_refs(
    program: &str,
    args: &[&str],
    log_sender: &Sender<String>,
) -> Result<(), UpdaterError> {
    let owned_args = args
        .iter()
        .map(|value| (*value).to_owned())
        .collect::<Vec<_>>();
    stream_checked(program, &owned_args, log_sender)
}

pub(super) fn heuristic_check(
    program: &str,
    args: &[&str],
    manager: &'static str,
) -> Result<Vec<PackageUpdate>, UpdaterError> {
    let owned_args = args
        .iter()
        .map(|value| (*value).to_owned())
        .collect::<Vec<_>>();
    let output = capture_command(program, &owned_args)?;
    Ok(heuristic_parse_updates(manager, &output.merged_text()))
}
