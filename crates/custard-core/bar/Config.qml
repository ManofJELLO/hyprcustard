pragma Singleton

import QtQuick
import Quickshell
import Quickshell.Hyprland
import Quickshell.Io

// The bar's settings, from bar.json, which hyprcustard's Bar page writes.
// The bar follows a change at once, without restarting.
Singleton {
    id: root

    property var values: ({})

    FileView {
        path: Quickshell.shellPath("bar.json")
        // Read before the first frame, so the bar never shows with defaults
        // and then jumps to its settings.
        blockLoading: true
        watchChanges: true
        onFileChanged: reload()
        onLoaded: {
            try {
                root.values = JSON.parse(text());
            } catch (e) {
                console.warn("bar.json could not be read: " + e);
            }
        }
    }

    function get(key, fallback) {
        const v = root.values[key];
        return v === undefined || v === null ? fallback : v;
    }

    readonly property var monitors: get("monitors", [])
    readonly property bool bottom: get("position", "top") === "bottom"
    readonly property int height: get("height", 36)
    readonly property int edgeMargin: get("edge_margin", 4)
    readonly property int sideMargin: get("side_margin", 0)
    readonly property int windowGap: get("window_gap", 0)
    readonly property real opacity: get("opacity", 0.8)
    readonly property string font: get("font", "")
    readonly property int fontSize: get("font_size", 11)
    readonly property bool glyphs: get("glyphs", true)
    // Neumorphic: raised and pressed-in shapes, like hyprcustard's window.
    readonly property bool neumorphic: get("style", "flat") === "neumorphic"
    // Neubrutalism: thick outlines and hard, offset shadows.
    readonly property bool brutal: get("style", "flat") === "neubrutalism"
    // Either of the two that draw their shapes with Neu.qml.
    readonly property bool shaped: root.neumorphic || root.brutal
    // "auto" keeps the color scheme's own light or dark; "light" and "dark"
    // ask for that one, which colors.json carries as well.
    readonly property string mode: get("mode", "auto")
    // Where the launcher and power menu open, and on which monitor ("" is
    // the focused one).
    readonly property string menuPosition: get("menu_position", "center")
    readonly property int menuMargin: get("menu_margin", 12)
    readonly property string menuMonitor: get("menu_monitor", "")
    readonly property int launcherWidth: get("launcher_width", 600)
    readonly property int launcherRows: get("launcher_rows", 8)
    readonly property bool launcherIcons: get("launcher_icons", true)
    // A grid of icons with their names under them, instead of a list.
    readonly property bool launcherGrid: get("launcher_layout", "list") === "grid"
    readonly property int launcherGridRows: get("launcher_grid_rows", 4)
    readonly property int launcherIconSize: get("launcher_icon_size", 48)
    readonly property var powerItems: get("power_items", ["lock", "logout", "suspend", "reboot", "shutdown"])
    readonly property bool powerColumn: get("power_layout", "row") === "column"
    readonly property bool powerIcons: get("power_icons", true)
    // The screen edges a menu is held to: none is the middle of the screen,
    // one is the middle of that side, two a corner.
    readonly property bool menuTop: root.menuPosition.startsWith("top")
    readonly property bool menuBottom: root.menuPosition.startsWith("bottom")
    readonly property bool menuLeft: root.menuPosition.endsWith("left")
    readonly property bool menuRight: root.menuPosition.endsWith("right")

    // The screen a menu opens on: the one the Bar page names while it is
    // plugged in, else the focused one.
    function menuScreen() {
        const want = root.menuMonitor !== "" ? root.menuMonitor
                   : Hyprland.focusedMonitor ? Hyprland.focusedMonitor.name : "";
        return Quickshell.screens.find(s => s.name === want)
            ?? Quickshell.screens.find(s => Hyprland.focusedMonitor && s.name === Hyprland.focusedMonitor.name)
            ?? Quickshell.screens[0] ?? null;
    }
    readonly property var modules: get("modules", ["launcher", "clock", "workspaces", "window", "media",
                                                   "tray", "volume", "bluetooth", "network", "cpu",
                                                   "memory", "cpu_temp", "gpu_temp", "power"])
    readonly property string clockFormat: get("clock_format", "ddd MM/dd  h:mm AP")
    readonly property bool workspacesPerMonitor: get("workspaces_per_monitor", false)
    readonly property var workspaceLabels: get("workspace_labels", [])
    readonly property string player: get("player", "")
    readonly property string mixer: get("mixer", "")
    readonly property string bluetoothApp: get("bluetooth", "")
    // Put before the command of an app that runs in a terminal, e.g. "kitty -e".
    readonly property string terminal: get("terminal", "")
    // The power menu's commands; an empty one leaves its button out.
    readonly property var power: get("power", ({
        lock: "loginctl lock-session",
        logout: "hyprctl dispatch 'hl.dsp.exit()' || hyprctl dispatch exit",
        suspend: "systemctl suspend",
        reboot: "systemctl reboot",
        shutdown: "systemctl poweroff"
    }))

    function has(id) { return root.modules.indexOf(id) >= 0; }
    function run(command) { if (command !== "") Quickshell.execDetached(["sh", "-c", command]); }
}
