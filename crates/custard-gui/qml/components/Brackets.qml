import QtQuick
import dev.hyprcustard.gui

// Four thin corner marks, like the frame of a heads-up display.
Item {
    id: frame

    property color color: Theme.wash(Theme.accent, 0.45)
    property int size: 12
    property real thickness: 1.5

    Repeater {
        model: 4

        Item {
            id: corner

            required property int index
            readonly property bool atRight: corner.index % 2 === 1
            readonly property bool atBottom: corner.index > 1

            x: corner.atRight ? frame.width - frame.size : 0
            y: corner.atBottom ? frame.height - frame.size : 0
            width: frame.size
            height: frame.size

            Rectangle {
                width: parent.width
                height: frame.thickness
                y: corner.atBottom ? parent.height - height : 0
                color: frame.color
            }
            Rectangle {
                width: frame.thickness
                height: parent.height
                x: corner.atRight ? parent.width - width : 0
                color: frame.color
            }
        }
    }
}
