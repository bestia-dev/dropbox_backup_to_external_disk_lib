// crossterm_cli_mod.rs

//! CLI interface using the crossterm crate

/// ansi color
pub const GREEN: crossterm::style::SetForegroundColor = crossterm::style::SetForegroundColor(crossterm::style::Color::Green);
/// ansi color
pub const YELLOW: crossterm::style::SetForegroundColor = crossterm::style::SetForegroundColor(crossterm::style::Color::Yellow);
/// ansi color
pub const RED: crossterm::style::SetForegroundColor = crossterm::style::SetForegroundColor(crossterm::style::Color::Red);
/// ansi color reset
pub const RESET: crossterm::style::ResetColor = crossterm::style::ResetColor;
/// ansi attribute
pub const BOLD: crossterm::style::SetAttribute = crossterm::style::SetAttribute(crossterm::style::Attribute::Bold);
