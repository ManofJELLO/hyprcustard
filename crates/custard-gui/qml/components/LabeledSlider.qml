import QtQuick
import QtQuick.Controls.Basic
import dev.hyprcustard.gui

// A slider with a label and an editable number beside it.
//
// `moved` fires while dragging (for a live preview) and `settled` fires once the
// drag ends or the number is typed — which is when the value is staged, so a
// drag stages once rather than two hundred times.
//
// `markValue` puts a tick on the track: where the value was before you started
// moving it. Set it only while that differs from the value, or it sits under
// the handle as noise.
Item {
    id: root

    property string label: ""
    property real from: 0
    property real to: 1
    property real stepSize: 0.01
    property real value: 0
    property bool integer: false
    // A reference point on the track, or null for none.
    property var markValue: null

    readonly property bool hasMark: root.markValue !== null && root.markValue !== undefined
                                    && root.markValue >= root.from && root.markValue <= root.to

    signal moved(real value)
    signal settled(real value)

    implicitHeight: Math.max(Theme.rowHeight, col.implicitHeight)
    implicitWidth: 320

    function format(v) {
        if (root.integer)
            return String(Math.round(v))
        var decimals = root.stepSize >= 1 ? 0 : (root.stepSize >= 0.1 ? 1 : (root.stepSize >= 0.01 ? 2 : 3))
        return v.toFixed(decimals)
    }

    Column {
        id: col
        width: parent.width
        spacing: 2

        Item {
            width: parent.width
            height: caption.implicitHeight

            Text {
                id: caption
                text: root.label
                color: Theme.fgDim
                font.pixelSize: Theme.fontSize
                anchors.left: parent.left
                anchors.verticalCenter: parent.verticalCenter
            }

            LineEdit {
                id: number
                anchors.right: parent.right
                width: 78
                height: 26
                implicitHeight: 26
                mono: true
                horizontalAlignment: Text.AlignRight
                text: root.format(root.value)
                onCommit: function (v) {
                    var parsed = parseFloat(v)
                    if (isNaN(parsed))
                        parsed = root.value
                    parsed = Math.max(root.from, Math.min(root.to, parsed))
                    root.value = parsed
                    root.settled(parsed)
                }
            }
        }

        Slider {
            id: slider
            width: parent.width
            from: root.from
            to: root.to
            stepSize: root.stepSize
            value: root.value
            snapMode: Slider.SnapAlways
            implicitHeight: 22

            onMoved: {
                root.value = value
                root.moved(value)
            }
            onPressedChanged: {
                if (!pressed)
                    root.settled(value)
            }

            background: Rectangle {
                id: track
                x: slider.leftPadding
                y: slider.topPadding + slider.availableHeight / 2 - height / 2
                width: slider.availableWidth
                height: 4
                radius: 2
                color: Theme.bgAlt
                border.width: 1
                border.color: Theme.border

                Rectangle {
                    width: slider.visualPosition * parent.width
                    height: parent.height
                    radius: 2
                    color: root.enabled ? Theme.accent : Theme.muted
                }

                // Where the value sits in the file. Positioned on the handle's
                // own travel, not the track's full width, so the mark and the
                // handle line up at every value rather than only in the middle.
                Rectangle {
                    visible: root.hasMark
                    width: 2
                    height: 12
                    radius: 1
                    color: Theme.muted
                    anchors.verticalCenter: parent.verticalCenter
                    x: {
                        var span = root.to - root.from
                        var at = span === 0 ? 0 : (root.markValue - root.from) / span
                        return at * (track.width - 14) + 7 - width / 2
                    }
                }
            }

            handle: Rectangle {
                x: slider.leftPadding + slider.visualPosition * (slider.availableWidth - width)
                y: slider.topPadding + slider.availableHeight / 2 - height / 2
                width: 14
                height: 14
                radius: 7
                color: slider.pressed ? Theme.accent : Theme.surfaceHi
                border.width: 1
                border.color: root.enabled ? Theme.accent : Theme.border
            }
        }
    }
}
