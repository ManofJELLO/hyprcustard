import QtQuick
import QtQuick.Controls.Basic
import dev.hyprcustard.gui

Button {
    id: control

    property bool primary: false
    property bool danger: false
    property string tooltip: ""
    // Badge-sized, for a button that sits inside a row of labels rather than
    // in a row of its own.
    property bool compact: false

    implicitHeight: control.compact ? 22 : Theme.rowHeight
    padding: control.compact ? 2 : Theme.gap
    leftPadding: control.compact ? Theme.gap : Theme.pad
    rightPadding: control.compact ? Theme.gap : Theme.pad
    hoverEnabled: true

    readonly property color _tint: danger ? Theme.error
                                          : (primary ? Theme.accent : Theme.border)

    background: Rectangle {
        radius: control.compact ? height / 2 : Theme.radiusSmall
        color: {
            if (!control.enabled)
                return "transparent"
            if (control.primary)
                return control.down ? Qt.darker(Theme.accent, 1.2) : Theme.accent
            if (control.down)
                return Theme.selection
            return control.hovered ? Theme.surfaceHi : "transparent"
        }
        border.width: 1
        border.color: control.enabled ? control._tint : Theme.border
        opacity: control.enabled ? 1.0 : 0.45

        Behavior on color {
            ColorAnimation { duration: 90 }
        }
    }

    contentItem: Text {
        text: control.text
        color: {
            if (!control.enabled)
                return Theme.muted
            if (control.primary)
                return Theme.onAccent
            if (control.danger)
                return Theme.error
            return Theme.fg
        }
        font.pixelSize: control.compact ? Theme.fontSizeSmall : Theme.fontSize
        font.weight: control.primary || control.compact ? Font.DemiBold : Font.Normal
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
        elide: Text.ElideRight
    }

    Tip {
        text: control.tooltip
        visible: control.tooltip !== "" && control.hovered
        delay: 500
    }
}
