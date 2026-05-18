#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandGroup {
    Session,
    Service,
    Runtime,
    Diagnostics,
    Context,
    Maintenance,
}

impl CommandGroup {
    pub const ALL: [Self; 6] = [
        Self::Session,
        Self::Service,
        Self::Runtime,
        Self::Diagnostics,
        Self::Context,
        Self::Maintenance,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Session => "Session",
            Self::Service => "Service",
            Self::Runtime => "Runtime",
            Self::Diagnostics => "Diagnostics",
            Self::Context => "Context",
            Self::Maintenance => "Maintenance",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CommandEntry {
    pub group: CommandGroup,
    pub title: &'static str,
    pub description: &'static str,
    pub example: &'static str,
    pub args: &'static [&'static str],
    pub extra_args_hint: Option<&'static str>,
    pub runnable: bool,
}

pub const COMMANDS: &[CommandEntry] = &[
    CommandEntry {
        group: CommandGroup::Session,
        title: "Open TUI",
        description: "Starts the interactive CTOX TUI for the selected installation.",
        example: "ctox tui",
        args: &["tui"],
        extra_args_hint: None,
        runnable: true,
    },
    CommandEntry {
        group: CommandGroup::Service,
        title: "Status",
        description: "Shows the detached CTOX service status snapshot.",
        example: "ctox status",
        args: &["status"],
        extra_args_hint: None,
        runnable: true,
    },
    CommandEntry {
        group: CommandGroup::Service,
        title: "Start Service",
        description: "Starts the detached CTOX background service.",
        example: "ctox start",
        args: &["start"],
        extra_args_hint: None,
        runnable: true,
    },
    CommandEntry {
        group: CommandGroup::Service,
        title: "Stop Service",
        description: "Stops the detached CTOX background service.",
        example: "ctox stop",
        args: &["stop"],
        extra_args_hint: None,
        runnable: true,
    },
    CommandEntry {
        group: CommandGroup::Runtime,
        title: "Boost Status",
        description: "Displays the current boost lease state.",
        example: "ctox boost status",
        args: &["boost", "status"],
        extra_args_hint: None,
        runnable: true,
    },
    CommandEntry {
        group: CommandGroup::Runtime,
        title: "Boost Stop",
        description: "Stops an active boost lease.",
        example: "ctox boost stop",
        args: &["boost", "stop"],
        extra_args_hint: None,
        runnable: true,
    },
    CommandEntry {
        group: CommandGroup::Diagnostics,
        title: "Version",
        description: "Prints the current installation version metadata.",
        example: "ctox version",
        args: &["version"],
        extra_args_hint: None,
        runnable: true,
    },
    CommandEntry {
        group: CommandGroup::Diagnostics,
        title: "Source Status",
        description: "Reports the detected CTOX source layout state.",
        example: "ctox source-status",
        args: &["source-status"],
        extra_args_hint: None,
        runnable: true,
    },
    CommandEntry {
        group: CommandGroup::Diagnostics,
        title: "Responses Proxy",
        description: "Runs the responses proxy in the foreground for the selected installation.",
        example: "ctox serve-responses-proxy",
        args: &["serve-responses-proxy"],
        extra_args_hint: Some("Optionale Host- oder Port-Flags"),
        runnable: true,
    },
    CommandEntry {
        group: CommandGroup::Context,
        title: "Export Chat Prompt",
        description: "Exports the current live prompt artifact using default paths.",
        example: "ctox chat-prompt-export",
        args: &["chat-prompt-export"],
        extra_args_hint: Some("Optionaler Zielpfad oder weitere Export-Flags"),
        runnable: true,
    },
    CommandEntry {
        group: CommandGroup::Context,
        title: "Context Retrieve",
        description: "Example entry point for current context inspection. Advanced flags follow later.",
        example: "ctox context-retrieve --mode current",
        args: &["context-retrieve", "--mode", "current"],
        extra_args_hint: Some("Weitere Filter oder IDs"),
        runnable: true,
    },
    CommandEntry {
        group: CommandGroup::Maintenance,
        title: "Update Status",
        description: "Shows updater channel and release state.",
        example: "ctox update status",
        args: &["update", "status"],
        extra_args_hint: None,
        runnable: true,
    },
    CommandEntry {
        group: CommandGroup::Maintenance,
        title: "Update Check",
        description: "Checks whether a newer release is available.",
        example: "ctox update check",
        args: &["update", "check"],
        extra_args_hint: None,
        runnable: true,
    },
    CommandEntry {
        group: CommandGroup::Maintenance,
        title: "Runtime Switch Template",
        description: "Template only. Requires a model id and preset, so it is documented but not click-runnable yet.",
        example: "ctox runtime switch <model> <quality|performance>",
        args: &[],
        extra_args_hint: Some("Modell-ID und Preset"),
        runnable: false,
    },
    CommandEntry {
        group: CommandGroup::Context,
        title: "LCM Template",
        description: "Template only. LCM commands require explicit db path and conversation identifiers.",
        example: "ctox lcm-show-continuity <db-path> <conversation-id>",
        args: &[],
        extra_args_hint: Some("DB-Pfad und Conversation-ID"),
        runnable: false,
    },
];

pub fn is_allowed_ctox_args(args: &[String]) -> bool {
    if args.iter().map(|value| value.as_str()).eq(["tui"]) {
        return true;
    }

    COMMANDS.iter().filter(|entry| entry.runnable).any(|entry| {
        entry
            .args
            .iter()
            .copied()
            .eq(args.iter().map(|value| value.as_str()))
    })
}
