//@ pragma UseQApplication
// hyprcustard's bar. hyprcustard writes this folder and rewrites it when it
// updates, so change the bar on its Bar page rather than here. To make the
// bar your own, copy the folder to another name under ~/.config/quickshell
// and start that one instead.
import QtQuick
import Quickshell

ShellRoot {
    // One bar per chosen monitor, or on every monitor when none is chosen.
    // Variants rebuilds a bar when its monitor goes away and comes back.
    Variants {
        model: Quickshell.screens.filter(s => Config.monitors.length === 0
                                              || Config.monitors.indexOf(s.name) >= 0)
        delegate: Bar {}
    }
}
