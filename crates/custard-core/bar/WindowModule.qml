import QtQuick
import Quickshell.Hyprland
import Quickshell.Wayland

// The focused window's title, centered; blank on an empty workspace.
// Hyprland.activeToplevel can stay null under a Lua config, so the title
// comes from the Wayland toplevel list instead.
Module {
    readonly property var win: ToplevelManager.activeToplevel
    readonly property bool emptyWorkspace: Hyprland.focusedWorkspace !== null
        && Hyprland.focusedWorkspace.toplevels.values.length === 0
    text: win && !emptyWorkspace ? (win.title ?? "") : ""
    visible: shown && text !== ""
    padLeft: 0
    padRight: 0
    maxTextWidth: 800
}
