//! Against the running compositor: every look option can be written back in
//! the Lua this crate emits, and reads back the same.
//!
//! It writes each option's *current* value, so the desktop does not change.
//! Ignored by default because it needs a live Hyprland session:
//!
//! cargo test -p custard-core --test look_live -- --ignored

use custard_core::look;

#[test]
#[ignore = "needs a running Hyprland"]
fn every_option_survives_a_write_and_read_back() {
    let before = look::read_live().expect("read the look from Hyprland");
    assert_eq!(before.len(), look::OPTIONS.len(), "Hyprland did not answer for every option");

    let all: Vec<(&str, &look::LookValue)> = before.iter().map(|(k, v)| (k.as_str(), v)).collect();
    look::apply_live(&all).expect("Hyprland accepted the emitted Lua");

    let after = look::read_live().expect("read the look again");
    for (key, value) in &before {
        assert!(value.same_as(&after[key]), "{key}: wrote {value:?}, read {:?}", after[key]);
    }
}
