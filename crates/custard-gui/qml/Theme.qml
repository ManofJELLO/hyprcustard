pragma Singleton

import QtQuick

// This file is part of the dev.hyprcustard.gui module, so the module's own
// types (Backend) are available without importing it. Importing it here would
// make the two singletons depend on each other through the module.

// Colors and metrics for the whole app.
//
// The engine decides *which* theme is in use and, for its own themes, what the
// colors are. The one it cannot answer is `system`, because reading a Qt
// palette needs Qt: those colors are derived here instead, from SystemPalette.
QtObject {
    id: theme

    // Main.qml keeps this in step with the backend. The singleton does not reach
    // for Backend itself: a singleton that imports its own module makes the
    // module's singletons depend on each other, which QML refuses to load.
    property string rawState: "{}"

    readonly property var _state: {
        try {
            return JSON.parse(theme.rawState)
        } catch (e) {
            return ({})
        }
    }
    readonly property var _theme: _state.theme || ({})
    readonly property var _c: _theme.colors || ({})

    // Follows the Qt palette, including a change of it while the app is open.
    readonly property SystemPalette _sys: SystemPalette {
        colorGroup: SystemPalette.Active
    }

    // True when the engine picked the theme that defers to Qt.
    readonly property bool _useSystem: _theme.system === true

    readonly property string name: _theme.name || "System (Qt)"
    readonly property real windowOpacity: _theme.opacity === undefined ? 1.0 : _theme.opacity

    // Qt has no "is this a dark theme" flag, so it is read off the window
    // color. Everything derived below leans on this to know which way to
    // push a shade.
    readonly property bool dark: _useSystem
                                 ? _luma(_sys.window) < 0.5
                                 : (_theme.dark === undefined ? true : _theme.dark)

    // --- the roles the rest of the app uses --------------------------------
    //
    // Mapped from the Qt roles that are dependable. SystemPalette does not
    // expose brightText, link or the toolTip pair at all, and `mid` is a light
    // gray even under a dark theme on at least one widely used style — so
    // hairlines and shades are mixed from window and windowText, which are
    // always present and always in the right relationship to each other.

    readonly property color bg: _useSystem ? _sys.window
                                           : (_c.bg || "#282828")
    readonly property color bgAlt: _useSystem ? _shade(_sys.window, dark ? -0.30 : -0.05)
                                              : (_c.bg_alt || "#1d2021")
    readonly property color surface: _useSystem ? _sys.base
                                                : (_c.surface || "#32302f")
    readonly property color surfaceHi: _useSystem ? _shade(_sys.base, dark ? 0.09 : -0.07)
                                                  : (_c.surface_hi || "#3c3836")
    readonly property color border: _useSystem ? _mix(_sys.windowText, _sys.window, 0.22)
                                               : (_c.border || "#504945")
    readonly property color fg: _useSystem ? _sys.windowText
                                           : (_c.fg || "#ebdbb2")
    readonly property color fgDim: _useSystem ? _mix(_sys.windowText, _sys.window, 0.75)
                                              : (_c.fg_dim || "#d5c4a1")
    readonly property color muted: _useSystem ? _mix(_sys.windowText, _sys.window, 0.50)
                                              : (_c.muted || "#928374")

    // The desktop's accent if the platform reports one (Qt 6.6+), otherwise
    // the selection color, which every theme sets.
    readonly property color accent: _useSystem ? _opaque(_sys.accent || _sys.highlight)
                                               : (_c.accent || "#83a598")
    readonly property color accentAlt: _useSystem ? _opaque(_sys.highlight)
                                                  : (_c.accent_alt || "#d3869b")
    readonly property color selection: _useSystem ? _mix(_opaque(_sys.highlight), _sys.window, 0.55)
                                                  : (_c.selection || "#3c3836")
    // highlightedText pairs with `highlight`, which is not necessarily what
    // `accent` ended up being, so this is chosen against the accent itself.
    readonly property color onAccent: _useSystem ? (_luma(accent) > 0.55 ? "#101010" : "#ffffff")
                                                 : (_c.on_accent || "#1d2021")

    // Qt has no palette role that means "this went well" or "this is wrong",
    // so these stay fixed, in a light and a dark variant that both read
    // against the background they land on.
    readonly property color ok: _useSystem ? (dark ? "#8fbf5f" : "#3f6f20")
                                           : (_c.ok || "#b8bb26")
    readonly property color warn: _useSystem ? (dark ? "#e0a93b" : "#8a5a00")
                                             : (_c.warn || "#fabd2f")
    readonly property color error: _useSystem ? (dark ? "#e56b5c" : "#a11")
                                              : (_c.error || "#fb4934")

    // --- color helpers -----------------------------------------------------

    // Perceived brightness, 0 to 1.
    function _luma(c) {
        return 0.299 * c.r + 0.587 * c.g + 0.114 * c.b
    }

    // `amount` of `a` against `b`: 0 is all `b`, 1 is all `a`.
    function _mix(a, b, amount) {
        return Qt.rgba(a.r * amount + b.r * (1 - amount),
                       a.g * amount + b.g * (1 - amount),
                       a.b * amount + b.b * (1 - amount),
                       1)
    }

    // Push a color towards white (positive) or black (negative).
    function _shade(c, amount) {
        return amount >= 0 ? _mix(Qt.rgba(1, 1, 1, 1), c, amount)
                           : _mix(Qt.rgba(0, 0, 0, 1), c, -amount)
    }

    // Drop any alpha. Some themes set a translucent highlight, which would
    // let whatever is behind a slider or a selected row show through it.
    function _opaque(c) {
        return Qt.rgba(c.r, c.g, c.b, 1)
    }

    // --- style ---------------------------------------------------------------
    //
    // Neumorphic (the default) or neubrutalism, from the app's settings.
    // Neubrutalism draws every raised shape as a flat face with a thick
    // outline over a hard shadow, offset to the bottom right and not
    // blurred; a pressed-in shape keeps its outline and loses its shadow.
    // Nothing shifts when pressed: the shape stays where the pointer left
    // it, only its shadow goes.

    readonly property bool brutal: (_state.settings || {}).style === "neubrutalism"
    // The outline, and the hard shadow under it, in one ink: near black on
    // a light background, and close to the text color on a dark one. A dark
    // shadow on a dark background does not show, and a shadow that does not
    // show is not this style.
    readonly property color ink: dark ? _mix(fg, bg, 0.85) : _shade(fg, -0.7)
    readonly property color drop: ink
    readonly property int inkWidth: 2

    // Black or white, whichever reads on `c`.
    function onColor(c) {
        return _luma(c) > 0.55 ? "#101010" : "#ffffff"
    }
    function opaque(c) {
        return _opaque(c)
    }

    // --- depth ---------------------------------------------------------------
    //
    // Shapes are raised out of the background or pressed into it: lit at the
    // top left, shadowed at the bottom right. Both shades come from the
    // background itself, so the look holds under any theme, light or dark.

    readonly property color shadowDark: _shade(bg, dark ? -0.55 : -0.20)
    readonly property color shadowLight: dark ? wash(_shade(bg, 0.12), 0.7)
                                              : Qt.rgba(1, 1, 1, 0.85)
    // The same pair, for the inner edges of a pressed-in shape.
    readonly property color innerDark: wash(_shade(bg, dark ? -0.6 : -0.25), dark ? 0.75 : 0.5)
    readonly property color innerLight: dark ? wash(_shade(bg, 0.12), 0.45)
                                             : Qt.rgba(1, 1, 1, 0.7)
    // The floor of a pressed-in shape.
    readonly property color well: _shade(bg, dark ? -0.16 : -0.05)
    // How far a raised shape stands out, and how deep a pressed one goes.
    readonly property int depth: 4
    readonly property int depthSmall: 3
    readonly property int depthInset: 4
    // A raised shape's shadow blur, as a share of its depth.
    readonly property real raisedBlur: 0.9
    // How far a raised shape's shadow reaches past its edge before it is
    // under half a level: the offset plus two blur widths.
    function shadowReach(d) {
        return brutal ? d : d + Math.ceil(d * raisedBlur * 2)
    }
    // Room around a page for its cards' shadows to fade out in before the
    // sidebar, the header or the window's edge cuts them off.
    readonly property int pageMargin: Math.max(pad, shadowReach(depth) + 2)
    // The least space between two cards: room for both their shadows, so
    // one card's shadow never falls on the next card.
    readonly property int cardGap: shadowReach(depth) * 2
    // The same for small raised tiles laid out in a section.
    readonly property int tileGap: shadowReach(depthSmall) * 2

    // Metrics. One place, so spacing stays consistent across pages.
    readonly property int pad: 16
    readonly property int gap: 10
    readonly property int gapSmall: 6
    readonly property int radius: brutal ? 6 : 12
    readonly property int radiusSmall: brutal ? 4 : 8
    readonly property int rowHeight: 34
    readonly property int sidebarWidth: 188
    readonly property int listWidth: 300

    // The widest a page's column of cards is allowed to get, centered in
    // whatever is left over. Past this a wider window only stretches the
    // fields inside it, and a dropdown reading "Toggle on the focused window"
    // two thousand pixels wide is not easier to use, only stranger.
    readonly property int contentMax: 1100

    readonly property int fontSize: 13
    readonly property int fontSizeSmall: 11
    readonly property int fontSizeLarge: 16
    readonly property int fontSizeTitle: 19

    readonly property string monoFamily: "monospace"

    // A translucent wash of a color, for badges and hover states.
    function wash(c, amount) {
        return Qt.rgba(c.r, c.g, c.b, amount === undefined ? 0.18 : amount)
    }
}
