// ANSI color codes for terminal output
// Colors are only applied when enabled (for CLI), not in WASM

const ANSI_RESET: &str = "\x1b[0m";
const ANSI_CYAN: &str = "\x1b[36m";
const ANSI_YELLOW: &str = "\x1b[33m";
const ANSI_RED: &str = "\x1b[31m";
const ANSI_GRAY: &str = "\x1b[90m";

pub fn cyan(enabled: bool, text: &str) -> String {
    if enabled {
        format!("{}{}{}", ANSI_CYAN, text, ANSI_RESET)
    } else {
        text.to_string()
    }
}

pub fn yellow(enabled: bool, text: &str) -> String {
    if enabled {
        format!("{}{}{}", ANSI_YELLOW, text, ANSI_RESET)
    } else {
        text.to_string()
    }
}

pub fn red(enabled: bool, text: &str) -> String {
    if enabled {
        format!("{}{}{}", ANSI_RED, text, ANSI_RESET)
    } else {
        text.to_string()
    }
}

pub fn gray(enabled: bool, text: &str) -> String {
    if enabled {
        format!("{}{}{}", ANSI_GRAY, text, ANSI_RESET)
    } else {
        text.to_string()
    }
}

pub fn cyan_if(enabled: bool, text: &str) -> String {
    cyan(enabled, text)
}

pub fn yellow_if(enabled: bool, text: &str) -> String {
    yellow(enabled, text)
}

