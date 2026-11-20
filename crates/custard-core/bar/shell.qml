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
        // Named apart from the bar's own properties: inside Bar { }, a bare
        // `launcher` would be the bar's own (empty) property, not this one.
        delegate: Bar {
            launcher: launcherWindow
            powerMenu: powerWindow
        }
    }

    // One of each, opened on the focused monitor or the one the Bar page names.
    Launcher { id: launcherWindow }
    PowerMenu { id: powerWindow }

    // `qs ipc -p <this folder> call launcher toggle`, and `... call power
    // toggle`: what a key binding runs.
    IpcHandler {
        target: "launcher"
        function toggle(): void { launcherWindow.toggle(); }
        function open(): void { launcherWindow.show(); }
        function close(): void { launcherWindow.hide(); }
        // Open with this already typed: `... call launcher search firefox`.
        function search(query: string): void { launcherWindow.showWith(query); }
    }
    IpcHandler {
        target: "power"
        function toggle(): void { powerWindow.toggle(); }
        function open(): void { powerWindow.show(); }
        function close(): void { powerWindow.hide(); }
    }
}
