import QtQuick
import dev.hyprcustard.gui

// A colour swatch that can be clicked. Transparency shows against a checker,
// so a half-transparent border colour looks half-transparent here too.
Rectangle {
    id: swatch

    // `#aarrggbb`, as the backend hands colours out.
    property string value: "#ff000000"
    property string tooltip: ""
    signal clicked()

    implicitWidth: 34
    implicitHeight: 26
    radius: Theme.radiusSmall
    color: "transparent"
    border.width: 1
    border.color: hover.hovered ? Theme.accent : Theme.border
    clip: true

    Grid {
        anchors.fill: parent
        anchors.margins: 1
        columns: Math.ceil(width / 6)
        clip: true
        Repeater {
            model: Math.ceil(swatch.width / 6) * Math.ceil(swatch.height / 6)
            Rectangle {
                required property int index
                width: 6
                height: 6
                readonly property int cols: Math.ceil(swatch.width / 6)
                color: ((index % cols) + Math.floor(index / cols)) % 2 === 0 ? "#cccccc" : "#888888"
            }
        }
    }

    Rectangle {
        anchors.fill: parent
        anchors.margins: 1
        radius: swatch.radius - 1
        color: swatch.value
    }

    HoverHandler {
        id: hover
        cursorShape: Qt.PointingHandCursor
    }
    TapHandler {
        onTapped: swatch.clicked()
    }
    Tip {
        visible: hover.hovered && swatch.tooltip !== ""
        text: swatch.tooltip
    }
}
