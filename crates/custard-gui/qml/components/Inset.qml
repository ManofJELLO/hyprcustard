import QtQuick
import dev.hyprcustard.gui

// A shape pressed into the background: a darker floor, shaded along its top
// and left edges and lit along its bottom. Fields, tracks and wells are drawn
// on one, and so is a button while it is held down.
Item {
    id: well

    property real radius: Theme.radiusSmall
    property color fill: Theme.well
    property bool glow: false
    property color glowColor: Theme.accent
    property color edge: "transparent"

    Rectangle {
        anchors.fill: parent
        radius: well.radius
        color: well.fill
    }
    // Top edge in shadow, bottom edge in light.
    Rectangle {
        anchors.fill: parent
        radius: well.radius
        gradient: Gradient {
            GradientStop { position: 0; color: Qt.rgba(0, 0, 0, Theme.dark ? 0.38 : 0.14) }
            GradientStop {
                position: Math.min(0.45, 9 / Math.max(1, well.height))
                color: "transparent"
            }
            GradientStop {
                position: Math.max(0.55, 1 - 5 / Math.max(1, well.height))
                color: "transparent"
            }
            GradientStop {
                position: 1
                color: Qt.rgba(Theme.shadowLight.r, Theme.shadowLight.g, Theme.shadowLight.b,
                               Theme.shadowLight.a * 0.5)
            }
        }
    }
    // Left edge in shadow.
    Rectangle {
        anchors.fill: parent
        radius: well.radius
        gradient: Gradient {
            orientation: Gradient.Horizontal
            GradientStop { position: 0; color: Qt.rgba(0, 0, 0, Theme.dark ? 0.30 : 0.10) }
            GradientStop {
                position: Math.min(0.5, 10 / Math.max(1, well.width))
                color: "transparent"
            }
            GradientStop { position: 1; color: "transparent" }
        }
    }
    Rectangle {
        anchors.fill: parent
        radius: well.radius
        color: "transparent"
        border.width: well.glow || well.edge.a > 0 ? 1 : 0
        border.color: well.glow ? Theme.wash(well.glowColor, 0.8) : well.edge
    }
}
