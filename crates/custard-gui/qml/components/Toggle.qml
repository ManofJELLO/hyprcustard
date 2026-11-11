import QtQuick
import QtQuick.Controls.Basic
import dev.hyprcustard.gui

Switch {
    id: control

    implicitHeight: 26
    font.pixelSize: Theme.fontSize

    indicator: Rectangle {
        implicitWidth: 38
        implicitHeight: 20
        x: control.leftPadding
        y: parent.height / 2 - height / 2
        radius: 10
        color: control.checked ? Theme.accent : Theme.bgAlt
        border.width: 1
        border.color: control.checked ? Theme.accent : Theme.border

        Behavior on color {
            ColorAnimation { duration: 110 }
        }

        Rectangle {
            x: control.checked ? parent.width - width - 2 : 2
            y: 2
            width: 16
            height: 16
            radius: 8
            color: control.checked ? Theme.onAccent : Theme.muted

            Behavior on x {
                NumberAnimation { duration: 110; easing.type: Easing.OutCubic }
            }
        }
    }

    contentItem: Text {
        text: control.text
        color: Theme.fgDim
        font: control.font
        leftPadding: control.indicator.width + Theme.gap
        verticalAlignment: Text.AlignVCenter
    }
}
