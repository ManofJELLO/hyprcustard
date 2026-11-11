import QtQuick
import dev.hyprcustard.gui

// A labelled row: caption on the left, one control on the right.
//
// The hint runs the full width underneath rather than sharing the caption
// column. A sentence of explanation squeezed into 150px becomes eight very
// short lines while the rest of the row sits empty.
Item {
    id: row

    property string label: ""
    property string hint: ""
    property int labelWidth: 150
    default property alias control: holder.data

    implicitHeight: head.height + (hintText.visible ? hintText.implicitHeight + 3 : 0)
    implicitWidth: labelWidth + Theme.gap + holder.childrenRect.width

    Item {
        id: head
        anchors {
            left: parent.left
            right: parent.right
            top: parent.top
        }
        height: Math.max(Theme.rowHeight, holder.childrenRect.height, caption.implicitHeight + 7)

        Text {
            id: caption
            text: row.label
            width: row.labelWidth
            anchors.left: parent.left
            anchors.top: parent.top
            anchors.topMargin: 7
            wrapMode: Text.WordWrap
            color: Theme.fgDim
            font.pixelSize: Theme.fontSize
        }

        Item {
            id: holder
            anchors {
                left: caption.right
                leftMargin: Theme.gap
                right: parent.right
                top: parent.top
            }
            implicitHeight: childrenRect.height
        }
    }

    Text {
        id: hintText
        text: row.hint
        visible: text !== ""
        anchors {
            left: parent.left
            right: parent.right
            top: head.bottom
            topMargin: 3
        }
        wrapMode: Text.WordWrap
        color: Theme.muted
        font.pixelSize: Theme.fontSizeSmall
    }
}
