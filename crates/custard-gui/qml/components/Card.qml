import QtQuick
import dev.hyprcustard.gui

// A panel raised out of the background. Content goes in `content`, which is
// laid out as a column.
Rectangle {
    id: card

    property string title: ""
    property string subtitle: ""
    default property alias content: column.data
    property int spacing: Theme.gap
    property alias columnItem: column

    color: "transparent"
    radius: Theme.radius
    implicitHeight: column.implicitHeight + Theme.pad * 2
        + (header.visible ? header.implicitHeight + Theme.gap : 0)
    implicitWidth: column.implicitWidth + Theme.pad * 2

    Raised {
        anchors.fill: parent
        z: -1
        radius: card.radius
    }
    Brackets {
        anchors.fill: parent
        anchors.margins: 7
        z: -1
    }

    Column {
        id: header
        visible: card.title !== "" || card.subtitle !== ""
        anchors { left: parent.left; right: parent.right; top: parent.top; margins: Theme.pad }
        spacing: 2

        Text {
            text: card.title
            visible: text !== ""
            color: Theme.fg
            font.pixelSize: Theme.fontSize + 1
            font.weight: Font.DemiBold
            font.capitalization: Font.AllUppercase
            font.letterSpacing: 3
        }
        Text {
            text: card.subtitle
            visible: text !== ""
            width: header.width
            wrapMode: Text.WordWrap
            color: Theme.muted
            font.pixelSize: Theme.fontSizeSmall
        }
    }

    Column {
        id: column
        anchors {
            left: parent.left
            right: parent.right
            top: header.visible ? header.bottom : parent.top
            topMargin: header.visible ? Theme.gap : Theme.pad
            leftMargin: Theme.pad
            rightMargin: Theme.pad
        }
        spacing: card.spacing
    }
}
