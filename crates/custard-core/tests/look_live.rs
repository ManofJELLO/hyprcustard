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

#[test]
#[ignore = "needs a running Hyprland"]
fn every_animation_and_curve_survives_a_write_and_read_back() {
    use custard_core::motion;

    let before = motion::read_live().expect("read the animations from Hyprland");
    assert!(before.curves.contains_key("default"));
    // Only curves the config defines: `default` is built in.
    let curves: std::collections::BTreeMap<_, _> = before
        .curves
        .iter()
        .filter(|(n, _)| *n != "default")
        .map(|(n, p)| (n.clone(), *p))
        .collect();
    let lua = motion::lua(&curves, &before.set);
    look::eval(&lua).expect("Hyprland accepted the emitted Lua");

    let after = motion::read_live().expect("read the animations again");
    assert_eq!(before.set.keys().collect::<Vec<_>>(), after.set.keys().collect::<Vec<_>>());
    assert!(motion::not_sticking(&before.set, &after).is_empty(), "{lua}");
    for (name, p) in &curves {
        let q = after.curves[name];
        assert!(p.iter().zip(q).all(|(a, b)| (a - b).abs() < 1e-3), "{name}: {p:?} became {q:?}");
    }
}
