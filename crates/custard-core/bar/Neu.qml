import QtQuick

// A shape raised out of the background or pressed into it, lit at the top
// left and shadowed at the bottom right, like hyprcustard's own window. The
// shading is one shader (neu.frag.qsb, beside this file), so the shadows
// are smooth rather than banded; without a GPU it is a flat shape.
//
// In neubrutalism (Config.brutal) it is a face with a thick outline over a
// hard shadow offset to the bottom right; sunk, it has no shadow. The
// shadow is drawn outside the face only, so a see-through face stays
// see-through. Nothing moves when it sinks: only the shadow goes.
Item {
    id: neu

    property bool sunk: false
    property real radius: 6
    property real depth: neu.sunk ? Theme.depth : Theme.depth
    property color fill: neu.sunk ? Theme.glass(Theme.well) : Theme.background

    // A pressed-in shape never shades more than a quarter of its height.
    readonly property real _depth: neu.sunk ? Math.min(neu.depth, Math.min(neu.width, neu.height) / 4)
                                            : neu.depth
    readonly property real _blur: Math.max(1, neu._depth * (neu.sunk ? 1.1 : 0.9))
    readonly property real _room: neu.sunk ? 0 : Math.ceil(neu._depth + neu._blur * 3)
    readonly property bool _software: GraphicsInfo.api === GraphicsInfo.Software
    readonly property bool _brutal: Config.brutal
    // The hard shadow's offset.
    readonly property int _offset: Math.round(neu.depth) + 1

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
        readonly property color ground: Theme.base

        fragmentShader: Qt.resolvedUrl("neu.frag.qsb")
    }

    Rectangle {
        visible: neu._software && !neu._brutal
        anchors.fill: parent
        radius: neu.radius
        color: neu.fill
    }

    // Neubrutalism's shadow, as an L past the right and bottom edges.
    Rectangle {
        visible: neu._brutal && !neu.sunk
        x: neu.width
        y: neu._offset
        width: neu._offset
        height: neu.height
        color: Theme.ink
    }
    Rectangle {
        visible: neu._brutal && !neu.sunk
        x: neu._offset
        y: neu.height
        width: neu.width - neu._offset
        height: neu._offset
        color: Theme.ink
    }
    Rectangle {
        visible: neu._brutal
        anchors.fill: parent
        radius: Math.min(neu.radius, 4)
        color: neu.fill
        border.width: Theme.inkWidth
        border.color: Theme.ink
    }
}
