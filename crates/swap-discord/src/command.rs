pub const SWAP_COMMAND: &str = "swap";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlashCommand {
    Join,
    Status,
    Claim,
    Submit,
    Skip,
    Pause,
    Resume,
    Extend,
    Leave,
    Help,
    Schedule,
    FillIn,
    Title,
    HostLock,
    HostReveal,
    HostStatus,
}

pub fn parse_swap_subcommand(value: &str) -> Option<SlashCommand> {
    Some(match value {
        "join" => SlashCommand::Join,
        "status" => SlashCommand::Status,
        "claim" => SlashCommand::Claim,
        "submit" => SlashCommand::Submit,
        "skip" => SlashCommand::Skip,
        "pause" => SlashCommand::Pause,
        "resume" => SlashCommand::Resume,
        "extend" => SlashCommand::Extend,
        "leave" => SlashCommand::Leave,
        "help" => SlashCommand::Help,
        "schedule" => SlashCommand::Schedule,
        "fillin" => SlashCommand::FillIn,
        "title" => SlashCommand::Title,
        "host-lock" => SlashCommand::HostLock,
        "host-reveal" => SlashCommand::HostReveal,
        "host-status" => SlashCommand::HostStatus,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn participant_commands_are_recognized() {
        assert_eq!(parse_swap_subcommand("join"), Some(SlashCommand::Join));
        assert_eq!(parse_swap_subcommand("status"), Some(SlashCommand::Status));
        assert_eq!(parse_swap_subcommand("help"), Some(SlashCommand::Help));
    }

    #[test]
    fn host_commands_are_recognized() {
        assert_eq!(
            parse_swap_subcommand("host-lock"),
            Some(SlashCommand::HostLock)
        );
        assert_eq!(
            parse_swap_subcommand("host-reveal"),
            Some(SlashCommand::HostReveal)
        );
    }

    #[test]
    fn unknown_commands_are_refused() {
        assert_eq!(parse_swap_subcommand("dance"), None);
    }
}
