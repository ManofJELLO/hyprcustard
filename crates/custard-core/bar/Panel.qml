import QtQuick

// The launcher's and power menu's card: the bar's background and shadow,
// with room around it for the shadow. Children go inside the card.
Item {
    id: panel

    // Room around the card for its shadow.
    readonly property int pad: 4
    default property alias content: card.data

    Repeater {
        model: [0.55, 0.3, 0.14, 0.05]
        delegate: Rectangle {
            required property real modelData
            required property int index
            anchors.fill: card
            anchors.margins: -(index + 1)
            radius: 2 + index + 1
            color: "transparent"
            border.width: 1
            border.color: Qt.rgba(Theme.shadow.r, Theme.shadow.g, Theme.shadow.b, modelData)
        }
    }

    Rectangle {
        id: card
        anchors { fill: parent; margins: panel.pad }
        radius: 2
        color: Theme.background
    }
}
