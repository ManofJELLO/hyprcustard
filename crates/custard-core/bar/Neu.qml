import QtQuick

// A shape raised out of the background or pressed into it, lit at the top
// left and shadowed at the bottom right, like hyprcustard's own window. The
// shading is one shader (neu.frag.qsb, beside this file), so the shadows
// are smooth rather than banded; without a GPU it is a flat shape.
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

    ShaderEffect {
        visible: !neu._software
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
        visible: neu._software
        anchors.fill: parent
        radius: neu.radius
        color: neu.fill
    }
}
