//! Starting hyprcustard at login, hidden in the tray.
//!
//! A managed block in the Hyprland config, like the wallpaper's: the user
//! turns it on and off on the Settings page, and turning it off takes the
//! block out again, leaving the rest of the file as it was.

use crate::block::{self, Markers};
use crate::error::Result;

/// The block's markers.
pub const MARKERS: Markers = Markers {
    begin: "-- >>> HyprCustard autostart (managed by hyprcustard) >>>",
    end: "-- <<< HyprCustard autostart (managed by hyprcustard) <<<",
    name: "HyprCustard autostart",
};

/// What the block holds.
pub const BODY: &str = "\
-- Starts hyprcustard in the tray at login. Turn it off on hyprcustard's
-- Settings page.
hl.on(\"hyprland.start\", function()
    hl.exec_cmd(\"hyprcustard --tray\")
end)
";

/// Whether the config starts hyprcustard at login.
pub fn enabled(text: &str) -> bool {
    matches!(block::split_with(text, &MARKERS), Ok(Some(_)))
}

/// The config with the block added or taken out.
pub fn set(text: &str, on: bool) -> Result<String> {
    if on {
        block::splice_with(text, BODY, &MARKERS)
    } else {
        block::remove_with(text, &MARKERS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn on_and_off_leave_the_config_as_it_was() {
        let mine = "hl.config({ general = { gaps_in = 4 } })\n";
        let on = set(mine, true).unwrap();
        assert!(enabled(&on) && !enabled(mine));
        assert!(on.starts_with(mine) && on.contains("hl.exec_cmd(\"hyprcustard --tray\")"));
        assert_eq!(set(&on, true).unwrap(), on, "on twice is one block");
        assert_eq!(set(&on, false).unwrap(), mine);
        assert_eq!(set(mine, false).unwrap(), mine);
    }
}
