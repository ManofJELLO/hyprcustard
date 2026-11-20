//@ pragma UseQApplication
// hyprcustard's bar. hyprcustard writes this folder and rewrites it when it
// updates, so change the bar on its Bar page rather than here. To make the
// bar your own, copy the folder to another name under ~/.config/quickshell
// and start that one instead.
import QtQuick
import Quickshell
import Quickshell.Io

ShellRoot {
    // One bar per chosen monitor, or on every monitor when none is chosen.
    // Variants rebuilds a bar when its monitor goes away and comes back.
    Variants {
        model: Quickshell.screens.filter(s => Config.monitors.length === 0
                                              || Config.monitors.indexOf(s.name) >= 0)
        delegate: Bar {
            launcher: launcher
            powerMenu: powerMenu
        }
    }

    // One of each, opened on the focused monitor.
    Launcher { id: launcher }
    PowerMenu { id: powerMenu }

    // `qs ipc -p <this folder> call launcher toggle`, and `... call power
    // toggle`: what a key binding runs.
    IpcHandler {
        target: "launcher"
        function toggle(): void { launcher.toggle(); }
        function open(): void { launcher.show(); }
        function close(): void { launcher.hide(); }
        // Open with this already typed: `... call launcher search firefox`.
        function search(query: string): void { launcher.showWith(query); }
    }
    IpcHandler {
        target: "power"
        function toggle(): void { powerMenu.toggle(); }
        function open(): void { powerMenu.show(); }
        function close(): void { powerMenu.hide(); }
    }
}
