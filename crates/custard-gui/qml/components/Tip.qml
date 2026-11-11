import QtQuick
import QtQuick.Controls.Basic
import dev.hyprcustard.gui

// A tooltip in the app's colours.
//
// The stock ToolTip draws itself from the *system* palette — toolTipBase,
// toolTipText and dark — which no theme here sets, so it comes up as a white
// box with blue text in the middle of a Gruvbox window. Everything else in the
// app styles its own background and contentItem; this does the same.
//
// The geometry is deliberately the same as QtQuick.Controls.Basic's ToolTip,
// including `margins`, which is what keeps a tip near the top of the window
// from being drawn off the edge: Qt flips it below the control instead.
ToolTip {
    id: tip

    // Long help text should wrap rather than run off the screen. Capping the
    // popup's width is safe because a Text's implicitWidth stays the width it
    // would need unwrapped, so this does not feed back into itself.
    readonly property int maxWidth: 360

    x: parent ? Math.round((parent.width - width) / 2) : 0
    y: -height - 3
    width: Math.min(implicitWidth, maxWidth)

    margins: 6
    padding: 7
    font.pixelSize: Theme.fontSizeSmall

    contentItem: Text {
        text: tip.text
        font: tip.font
        color: Theme.fg
        wrapMode: Text.Wrap
    }

    background: Rectangle {
        color: Theme.bgAlt
        border.width: 1
        border.color: Theme.border
        radius: Theme.radiusSmall
    }
}
