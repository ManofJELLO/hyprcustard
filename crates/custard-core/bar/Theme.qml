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

    // The scheme's own colors, or the other mode's when the Bar page asks
    // for light or dark and the scheme is the other one. A colors.json from
    // before the choice has no other set, and keeps its own.
    readonly property var colors: {
        const own = root.c;
        const want = Config.mode;
        const flip = (want === "light" && own.dark === true) || (want === "dark" && own.dark === false);
        return flip && own.other ? own.other : own;
    }

    // Gruvbox Dark until a scheme is applied.
    function pick(name, fallback) { return root.colors[name] ?? fallback; }

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
    // The launcher's and power menu's selected row, and their quieter text.
    readonly property color selection: pick("surface_container_high", "#3c3836")
    readonly property color dim: pick("on_surface_variant", "#a89984")

    // A color as see-through as the bar's background, for the shapes on it.
    function glass(c) { return Qt.rgba(c.r, c.g, c.b, Config.opacity); }

    // Neumorphic shading, worked out from the background as hyprcustard's
    // own window does: a dark shadow and a light one, and the floor of a
    // pressed-in shape.
    function mix(a, b, amount) {
        return Qt.rgba(a.r * amount + b.r * (1 - amount), a.g * amount + b.g * (1 - amount),
                       a.b * amount + b.b * (1 - amount), 1);
    }
    function shade(c, amount) {
        return amount >= 0 ? mix(Qt.rgba(1, 1, 1, 1), c, amount) : mix(Qt.rgba(0, 0, 0, 1), c, -amount);
    }
    function wash(c, a) { return Qt.rgba(c.r, c.g, c.b, a); }
    readonly property bool dark: 0.299 * base.r + 0.587 * base.g + 0.114 * base.b < 0.5
    readonly property color shadowDark: shade(base, dark ? -0.55 : -0.20)
    readonly property color shadowLight: dark ? wash(shade(base, 0.12), 0.7) : Qt.rgba(1, 1, 1, 0.85)
    readonly property color innerDark: wash(shade(base, dark ? -0.6 : -0.25), dark ? 0.75 : 0.5)
    readonly property color innerLight: dark ? wash(shade(base, 0.12), 0.45) : Qt.rgba(1, 1, 1, 0.7)
    readonly property color well: shade(base, dark ? -0.16 : -0.05)
    // How far a shape stands out: the bar and the menus, and the small
    // shapes on them.
    readonly property int depth: 3
    readonly property int depthSmall: 2
    // How far a shadow reaches past its shape before it is under half a
    // level: room to leave around a shape, and half the space between two.
    function reach(d) { return d + Math.ceil(d * 0.9 * 2); }
}
