import QtQuick
import QtQuick.Effects
import dev.hyprcustard.gui

// A shape raised out of the background: lit at the top left, shadowed at the
// bottom right. Cards, buttons and handles are drawn on one.
//
// `glow` puts a soft colored light around it, for what is focused or on.
Item {
    id: shape

    property real radius: Theme.radius
    property real depth: Theme.depth
    property color fill: Theme.bg
    property bool glow: false
    property color glowColor: Theme.accent
    // A hairline, when the shape needs one besides the glow's.
    property color edge: "transparent"

    RectangularShadow {
        visible: shape.depth > 0
        anchors.fill: parent
        radius: shape.radius
        offset: Qt.vector2d(shape.depth, shape.depth)
        blur: shape.depth * 2.6
        color: Theme.shadowDark
    }
    RectangularShadow {
        visible: shape.depth > 0
        anchors.fill: parent
        radius: shape.radius
        offset: Qt.vector2d(-shape.depth * 0.7, -shape.depth * 0.7)
        blur: shape.depth * 2.4
        color: Theme.shadowLight
    }
    RectangularShadow {
        visible: shape.glow
        anchors.fill: parent
        radius: shape.radius
        blur: 14
        color: Theme.glow(shape.glowColor)
    }
    Rectangle {
        anchors.fill: parent
        radius: shape.radius
        gradient: Gradient {
            GradientStop { position: 0; color: Qt.tint(shape.fill, Qt.rgba(1, 1, 1, 0.035)) }
            GradientStop { position: 1; color: Qt.tint(shape.fill, Qt.rgba(0, 0, 0, 0.06)) }
        }
        border.width: shape.glow || shape.edge.a > 0 ? 1 : 0
        border.color: shape.glow ? Theme.wash(shape.glowColor, 0.85) : shape.edge
    }
}
