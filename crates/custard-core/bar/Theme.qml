pragma Singleton

import QtQuick
import Quickshell
import Quickshell.Io

// Colors from colors.json, which hyprcustard rewrites every time a color
// scheme is applied. The bar follows it at once, without restarting.
Singleton {
    id: root

    property var c: ({})

    FileView {
        path: Quickshell.shellPath("colors.json")
        // Read before the first frame, so the bar never flashes other colors.
        blockLoading: true
        watchChanges: true
        onFileChanged: reload()
        onLoaded: {
            try {
                root.c = JSON.parse(text());
            } catch (e) {
                console.warn("colors.json could not be read: " + e);
            }
        }
    }

    // Gruvbox Dark until a scheme is applied.
    function pick(name, fallback) { return root.c[name] ?? fallback; }

    readonly property string font: Config.font !== "" ? Config.font : Qt.application.font.family
    readonly property int fontSize: Config.fontSize

    readonly property color base: pick("background", "#282828")
    readonly property color background: Qt.rgba(base.r, base.g, base.b, Config.opacity)
    readonly property color hardBackground: pick("surface_container_lowest", "#1d2021")
    readonly property color foreground: pick("on_surface", "#ebdbb2")
    readonly property color aqua: pick("base0C", "#8ec07c")
    readonly property color red: pick("base08", "#fb4934")
    readonly property color yellow: pick("base0A", "#fabd2f")
    readonly property color green: pick("base0B", "#b8bb26")
    readonly property color blue: pick("base0D", "#83a598")
    readonly property color shadow: pick("shadow", "#000000")
}
