import QtQuick
import dev.hyprcustard.gui

// A small tinted label: a tag name, a warning, a state.
Rectangle {
    id: badge

    property string text: ""
    property color tint: Theme.accent
    property bool solid: false

    implicitWidth: label.implicitWidth + 14
    implicitHeight: 20
    radius: 10
    color: badge.solid ? badge.tint : Theme.wash(badge.tint, Theme.dark ? 0.2 : 0.16)
    border.width: 1
    border.color: Theme.wash(badge.tint, 0.55)

    Text {
        id: label
        anchors.centerIn: parent
        text: badge.text
        color: badge.solid ? Theme.onAccent : badge.tint
        font.pixelSize: Theme.fontSizeSmall
        font.weight: Font.DemiBold
    }
}
