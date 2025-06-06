use clap::{CommandFactory, Parser, command};
use clap_complete::{Generator, shells};

#[derive(Debug, Parser)]
pub struct CompletionsCommand {
    /// The shell to generate completions for
    #[command(subcommand)]
    pub shell: Shell,
}

#[expect(clippy::enum_variant_names)] // powershell is powershell, not "power"
#[derive(Debug, Parser, Clone, Copy)]
pub enum Shell {
    /// Generate completions for bash
    Bash,
    /// Generate completions for fish
    Fish,
    /// Generate completions for zsh
    Zsh,
    /// Generate completions for elvish
    Elvish,
    /// Generate completions for powershell
    #[command(name = "powershell")]
    PowerShell,
}

impl Shell {
    pub fn completions(self) {
        match self {
            Shell::Bash => Self::completions_for_shell(shells::Bash),
            Shell::Fish => Self::completions_for_shell(shells::Fish),
            Shell::Zsh => Self::completions_for_shell(shells::Zsh),
            Shell::Elvish => Self::completions_for_shell(shells::Elvish),
            Shell::PowerShell => Self::completions_for_shell(shells::PowerShell),
        }
    }

    fn completions_for_shell(generator: impl Generator) {
        clap_complete::generate(
            generator,
            &mut super::Args::command(),
            "grafbase",
            &mut std::io::stdout(),
        );
    }
}
