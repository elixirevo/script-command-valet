use std::io::{self, IsTerminal};

pub fn is_enabled(no_input: bool) -> bool {
    is_enabled_for(no_input, io::stdin().is_terminal())
}

fn is_enabled_for(no_input: bool, stdin_is_terminal: bool) -> bool {
    !no_input && stdin_is_terminal
}

#[cfg(test)]
mod tests {
    use super::is_enabled_for;

    #[test]
    fn enables_prompts_only_for_terminal_input() {
        assert!(is_enabled_for(false, true));
        assert!(!is_enabled_for(true, true));
        assert!(!is_enabled_for(false, false));
        assert!(!is_enabled_for(true, false));
    }
}
