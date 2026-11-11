import QtQuick
import dev.hyprcustard.gui

// A panel. Content goes in `content`, which is laid out as a column.
Rectangle {
    id: card

    property string title: ""
    property string subtitle: ""
    default property alias content: column.data
    property int spacing: Theme.gap
    property alias columnItem: column

    color: Theme.surface
    radius: Theme.radius
    border.width: 1
    border.color: Theme.border
    implicitHeight: column.implicitHeight + Theme.pad * 2
        + (header.visible ? header.implicitHeight + Theme.gap : 0)
    implicitWidth: column.implicitWidth + Theme.pad * 2

    Column {
        id: header
        visible: card.title !== "" || card.subtitle !== ""
        anchors { left: parent.left; right: parent.right; top: parent.top; margins: Theme.pad }
        spacing: 2

        Text {
            text: card.title
            visible: text !== ""
            color: Theme.fg
            font.pixelSize: Theme.fontSizeLarge
            font.weight: Font.DemiBold
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
