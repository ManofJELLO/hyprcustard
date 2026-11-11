import QtQuick
import dev.hyprcustard.gui

// What a list says when it has nothing in it yet.
Column {
    id: empty

    property string title: ""
    property string body: ""

    spacing: Theme.gapSmall

    Text {
        text: empty.title
        color: Theme.fgDim
        font.pixelSize: Theme.fontSizeLarge
        width: empty.width
        horizontalAlignment: Text.AlignHCenter
        wrapMode: Text.WordWrap
    }

    Text {
        text: empty.body
        color: Theme.muted
        font.pixelSize: Theme.fontSize
        width: empty.width
        horizontalAlignment: Text.AlignHCenter
        wrapMode: Text.WordWrap
    }
}
