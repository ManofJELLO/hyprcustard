import QtQuick
import QtQuick.Controls.Basic
import dev.hyprcustard.gui

Switch {
    id: control

    implicitHeight: 26
    font.pixelSize: Theme.fontSize

    // A pressed-in track with a raised knob, which lights up when on.
    indicator: Inset {
        implicitWidth: 42
        implicitHeight: 22
        x: control.leftPadding
        y: parent.height / 2 - height / 2
        radius: height / 2
        glow: control.checked
        opacity: control.enabled ? 1.0 : 0.45

        Raised {
            x: control.checked ? parent.width - width - 3 : 3
            y: 3
            width: 16
            height: 16
            radius: 8
            depth: 2
            fill: control.checked ? Theme.accent : Theme.bg
            glow: control.checked

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
