import QtQuick
import dev.hyprcustard.gui

// A shape raised out of the background, lit at the top left and shadowed at
// the bottom right. Cards, buttons and handles are drawn on one. With `sunk`
// it is pressed into the background instead, which is how a held button, a
// field or the item that is chosen looks.
//
// The shading is one shader (shaders/neu.frag), so shadows are smooth rather
// than banded. The software renderer cannot run it and gets a flat shape.
//
// In the neubrutalism style (Theme.brutal) it is a flat face with a thick
// outline over a hard shadow `depth` pixels to the bottom right; sunk, it
// keeps the outline and has no shadow. The face never moves.
Item {
    id: neu

    property bool sunk: false
    property real radius: Theme.radius
    property real depth: neu.sunk ? Theme.depthInset : Theme.depth
    property color fill: neu.sunk ? Theme.well : (Theme.brutal ? Theme.surface : Theme.bg)
    // A hairline in the accent, for what has the keyboard's focus.
    property bool ring: false
    property color ringColor: Theme.accent
    // A hairline in any color, when the shape needs one besides the ring.
    property color edge: "transparent"

    // A pressed-in shape never shades more than a quarter of its own height,
    // so a thin slider track does not go entirely dark.
    readonly property real _depth: neu.sunk
        ? Math.min(neu.depth, Math.min(neu.width, neu.height) / 4)
        : neu.depth
    readonly property real _blur: Math.max(1, neu._depth * (neu.sunk ? 1.1 : Theme.raisedBlur))
    // Room around a raised shape for its shadows to fade out in.
    readonly property real _room: neu.sunk ? 0 : Math.ceil(neu._depth + neu._blur * 3)
    readonly property bool _software: GraphicsInfo.api === GraphicsInfo.Software
    readonly property bool _brutal: Theme.brutal

    ShaderEffect {
        visible: !neu._software && !neu._brutal
        x: -neu._room
        y: -neu._room
        width: neu.width + neu._room * 2
        height: neu.height + neu._room * 2

        readonly property size size: Qt.size(width, height)
        readonly property rect shape: Qt.rect(neu._room, neu._room, neu.width, neu.height)
        readonly property real radius: neu.radius
        readonly property real offset: neu.sunk ? neu._depth * 0.6 : neu._depth
        readonly property real blur: neu._blur
        readonly property real sunk: neu.sunk ? 1 : 0
        readonly property color fill: neu.fill
        readonly property color dark: neu.sunk ? Theme.innerDark : Theme.shadowDark
        readonly property color light: neu.sunk ? Theme.innerLight : Theme.shadowLight
        readonly property color ground: Theme.bg

        fragmentShader: "qrc:/qt/qml/dev/hyprcustard/gui/shaders/neu.frag.qsb"
    }

    // The flat stand-in for the software renderer.
    Rectangle {
        visible: neu._software && !neu._brutal
        anchors.fill: parent
        radius: neu.radius
        color: neu.fill
        border.width: 1
        border.color: Theme.wash(Theme.fg, 0.08)
    }

    // Neubrutalism: the hard shadow, then the outlined face. An `edge` is
    // the outline's color, at full strength.
    // A faded shape (a disabled button) lies flat: its shadow would show
    // through the face.
    Rectangle {
        visible: neu._brutal && !neu.sunk && neu.depth > 0 && neu.opacity >= 1
        x: Math.round(neu.depth)
        y: Math.round(neu.depth)
        width: neu.width
        height: neu.height
        radius: neu.radius
        color: Theme.drop
    }
    Rectangle {
        visible: neu._brutal
        anchors.fill: parent
        radius: neu.radius
        color: neu.fill
        border.width: Theme.inkWidth
        border.color: neu.edge.a > 0 ? Theme.opaque(neu.edge) : Theme.ink
    }

    Rectangle {
        visible: neu.ring || (neu.edge.a > 0 && !neu._brutal)
        // Outside the outline, in neubrutalism, so the two do not merge.
        anchors.fill: parent
        anchors.margins: neu._brutal ? -(Theme.inkWidth + 2) : 0
        radius: neu.radius + (neu._brutal ? Theme.inkWidth + 2 : 0)
        color: "transparent"
        border.width: neu._brutal ? 2 : 1
        border.color: neu.ring ? (neu._brutal ? neu.ringColor : Theme.wash(neu.ringColor, 0.7)) : neu.edge
    }
}
