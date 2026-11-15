import QtQuick
import QtQuick.Controls.Basic
import dev.hyprcustard.gui

Switch {
    id: control

    // Shown on hover, when set.
    property string tooltip: ""

    implicitHeight: 26
    font.pixelSize: Theme.fontSize

    // A pressed-in track with a raised knob. On, the track fills with the
    // accent and the knob takes it too.
    indicator: Inset {
        implicitWidth: 42
        implicitHeight: 22
        x: control.leftPadding
        y: parent.height / 2 - height / 2
        radius: height / 2
        fill: control.checked ? Theme.wash(Theme.accent, 0.45) : Theme.well
        opacity: control.enabled ? 1.0 : 0.45

        Raised {
            x: control.checked ? parent.width - width - 3 : 3
            y: 3
            width: 16
            height: 16
            radius: 8
            depth: 2
            fill: control.checked ? Theme.accent : Theme.bg

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

    Tip {
        text: control.tooltip
        visible: control.tooltip !== "" && control.hovered
    }
}
