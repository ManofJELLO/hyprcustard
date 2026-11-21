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
    readonly property bool _filled: Theme.brutal && (control.primary || control.danger) && control.enabled
    readonly property real _radius: control.compact ? height / 2 : Theme.radiusSmall

    // Raised, and pressed in while held. A primary or danger button says so
    // with the color of its text.
    background: Raised {
        implicitHeight: control.implicitHeight
        opacity: control.enabled ? 1.0 : 0.45
        sunk: control._sunk
        radius: control._radius
        depth: control._sunk ? (control.compact ? 2 : Theme.depthSmall)
                             : (control.compact ? 2 : (control.hovered ? Theme.depthSmall + 1
                                                                       : Theme.depthSmall))
        ring: control.visualFocus
        ringColor: control._tint
        // Neubrutalism fills a primary or danger button with its color.
        fill: control._filled ? control._tint
              : (control._sunk ? Theme.well : (Theme.brutal ? Theme.surface : Theme.bg))
    }

    contentItem: Text {
        text: control.text
        color: {
            if (!control.enabled)
                return Theme.muted
            if (control._filled)
                return Theme.onColor(control._tint)
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
