import QtQuick

// The launcher's and power menu's card, in the bar's style: flat with a thin
// shadow, or raised (neumorphic). It leaves room around the card for its
// shadow. Children go inside the card.
Item {
    id: panel

    // Room around the card for its shadow.
    readonly property int pad: Config.neumorphic ? Theme.reach(Theme.depth) + 1 : 4
    default property alias content: inner.data

    Item {
        id: card
        anchors { fill: parent; margins: panel.pad }

        Repeater {
            model: Config.neumorphic ? [] : [0.55, 0.3, 0.14, 0.05]
            delegate: Rectangle {
                required property real modelData
                required property int index
                anchors.fill: parent
                anchors.margins: -(index + 1)
                radius: 2 + index + 1
                color: "transparent"
                border.width: 1
                border.color: Qt.rgba(Theme.shadow.r, Theme.shadow.g, Theme.shadow.b, modelData)
            }
        }

        Rectangle {
            visible: !Config.neumorphic
            anchors.fill: parent
            radius: 2
            color: Theme.background
        }

        Neu {
            visible: Config.neumorphic
            anchors.fill: parent
            radius: 10
        }

        Item {
            id: inner
            anchors.fill: parent
        }
    }
}
