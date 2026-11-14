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

    readonly property color _tint: danger ? Theme.error : Theme.accent
    readonly property bool _sunk: control.down || control.checked
    readonly property real _radius: control.compact ? height / 2 : Theme.radiusSmall

    // Raised, and pressed in while held. A primary or danger button carries
    // its color on its edge, and the primary one glows while it can be
    // pressed.
    background: Item {
        implicitHeight: control.implicitHeight
        opacity: control.enabled ? 1.0 : 0.45

        Raised {
            anchors.fill: parent
            visible: !control._sunk
            radius: control._radius
            depth: control.compact ? 2 : (control.hovered ? Theme.depthSmall + 1 : Theme.depthSmall)
            glow: control.primary && control.enabled
            glowColor: control._tint
            edge: control.danger ? Theme.wash(Theme.error, 0.7) : "transparent"
        }
        Inset {
            anchors.fill: parent
            visible: control._sunk
            radius: control._radius
            glow: control.primary || control.danger
            glowColor: control._tint
        }
    }

    contentItem: Text {
        text: control.text
        color: {
            if (!control.enabled)
                return Theme.muted
            if (control.primary)
                return Theme.accent
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
