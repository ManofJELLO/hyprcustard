import QtQuick
import Quickshell

// Hover tooltip shown under a bar item. It opens in its own popup surface,
// so it never pushes or covers anything in the bar itself.
PopupWindow {
    id: tip

    required property Item target
    property bool hovered: false
    property string text: ""
    property bool rich: false

    anchor.item: target
    anchor.rect.x: 0
    anchor.rect.y: Config.bottom ? 0 : target.height
    anchor.rect.width: target.width
    anchor.rect.height: 0
    anchor.edges: Config.bottom ? Edges.Top : Edges.Bottom
    anchor.gravity: Config.bottom ? Edges.Top : Edges.Bottom

    visible: shown && text !== ""
    color: "transparent"
    implicitWidth: card.width + 16
    implicitHeight: card.height + 16

    property bool shown: false
    onHoveredChanged: {
        if (hovered) delay.restart();
        else { delay.stop(); shown = false; }
    }
    Timer { id: delay; interval: 400; onTriggered: tip.shown = true }

    Rectangle {
        anchors.fill: card
        anchors.margins: -1
        radius: 3
        color: "transparent"
        border.width: 1
        border.color: Qt.rgba(Theme.shadow.r, Theme.shadow.g, Theme.shadow.b, 0.6)
    }
    Rectangle {
        id: card
        x: 8; y: 8
        width: label.implicitWidth + 8
        height: label.implicitHeight + 8
        radius: 2
        color: Theme.background
        Text {
            id: label
            anchors.centerIn: parent
            text: tip.text
            textFormat: tip.rich ? Text.RichText : Text.PlainText
            color: Theme.foreground
            font.family: Theme.font
            font.pixelSize: Theme.fontSize
        }
    }
}
