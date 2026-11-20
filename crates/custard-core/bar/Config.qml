pragma Singleton

import QtQuick
import Quickshell
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
    readonly property var modules: get("modules", ["clock", "workspaces", "window", "media", "tray",
                                                   "volume", "bluetooth", "network", "cpu", "memory"])
    readonly property string clockFormat: get("clock_format", "ddd MM/dd  h:mm AP")
    readonly property bool workspacesPerMonitor: get("workspaces_per_monitor", false)
    readonly property var workspaceLabels: get("workspace_labels", [])
    readonly property string player: get("player", "")
    readonly property string mixer: get("mixer", "")
    readonly property string bluetoothApp: get("bluetooth", "")

    function has(id) { return root.modules.indexOf(id) >= 0; }
    function run(command) { if (command !== "") Quickshell.execDetached(["sh", "-c", command]); }
}
