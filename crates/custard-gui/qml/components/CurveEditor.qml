import QtQuick
import dev.hyprcustard.gui

// A cubic bezier easing curve with its two control points to drag.
//
// Time runs left to right, progress bottom to top. The dashed box is 0–1 on
// both; a point above it overshoots, which is how a curve bounces.
Item {
    id: editor

    // x0, y0, x1, y1 — as the backend holds a curve.
    property var points: [0, 0, 1, 1]
    property bool editable: true
    // While a handle is held, this is what is drawn instead of `points`.
    property var dragging: null
    readonly property var shown: editor.dragging !== null ? editor.dragging : editor.points

    // Each drag step, for anything that wants to follow along.
    signal moved(var points)
    // A handle let go: the curve to store.
    signal committed(var points)

    readonly property real yMin: -0.6
    readonly property real yMax: 1.6
    readonly property real pad: 14

    implicitWidth: 320
    implicitHeight: 240

    function px(x) {
        return editor.pad + x * (editor.width - 2 * editor.pad)
    }
    function py(y) {
        return editor.pad + (editor.yMax - y) / (editor.yMax - editor.yMin)
            * (editor.height - 2 * editor.pad)
    }
    function toX(p) {
        return Math.max(0, Math.min(1, (p - editor.pad) / (editor.width - 2 * editor.pad)))
    }
    function toY(p) {
        var y = editor.yMax - (p - editor.pad) / (editor.height - 2 * editor.pad)
            * (editor.yMax - editor.yMin)
        return Math.max(editor.yMin, Math.min(editor.yMax, y))
    }
    function round(v) {
        return Math.round(v * 100) / 100
    }

    onShownChanged: plot.requestPaint()
    onWidthChanged: plot.requestPaint()
    onHeightChanged: plot.requestPaint()

    Rectangle {
        anchors.fill: parent
        radius: Theme.radiusSmall
        color: Theme.bgAlt
        border.width: 1
        border.color: Theme.border
    }

    Canvas {
        id: plot
        anchors.fill: parent

        onPaint: {
            var ctx = getContext("2d")
            ctx.reset()
            var p = editor.shown
            var x0 = editor.px(0), x1 = editor.px(1)
            var y0 = editor.py(0), y1 = editor.py(1)

            // The unit box.
            ctx.strokeStyle = Theme.muted
            ctx.lineWidth = 1
            ctx.setLineDash([3, 4])
            ctx.strokeRect(x0, y1, x1 - x0, y0 - y1)
            ctx.setLineDash([])

            // Handle arms.
            ctx.strokeStyle = Theme.fgDim
            ctx.beginPath()
            ctx.moveTo(x0, y0)
            ctx.lineTo(editor.px(p[0]), editor.py(p[1]))
            ctx.moveTo(x1, y1)
            ctx.lineTo(editor.px(p[2]), editor.py(p[3]))
            ctx.stroke()

            // The curve.
            ctx.strokeStyle = Theme.accent
            ctx.lineWidth = 2.5
            ctx.beginPath()
            ctx.moveTo(x0, y0)
            ctx.bezierCurveTo(editor.px(p[0]), editor.py(p[1]),
                              editor.px(p[2]), editor.py(p[3]), x1, y1)
            ctx.stroke()
        }
    }

    Repeater {
        model: 2

        Rectangle {
            id: knob

            required property int index

            width: 16
            height: 16
            radius: 8
            x: editor.px(editor.shown[knob.index * 2]) - width / 2
            y: editor.py(editor.shown[knob.index * 2 + 1]) - height / 2
            color: knobArea.pressed ? Theme.accent : Theme.surfaceHi
            border.width: 2
            border.color: Theme.accent
            visible: editor.editable

            MouseArea {
                id: knobArea
                anchors.fill: parent
                anchors.margins: -6
                preventStealing: true
                cursorShape: Qt.SizeAllCursor
                hoverEnabled: true

                function place(mouse) {
                    var at = mapToItem(editor, mouse.x, mouse.y)
                    var next = (editor.dragging || editor.points).slice()
                    next[knob.index * 2] = editor.round(editor.toX(at.x))
                    next[knob.index * 2 + 1] = editor.round(editor.toY(at.y))
                    editor.dragging = next
                    editor.moved(next)
                }

                onPressed: function (mouse) { editor.dragging = editor.points.slice() }
                onPositionChanged: function (mouse) {
                    if (pressed)
                        place(mouse)
                }
                onReleased: function (mouse) {
                    var done = editor.dragging
                    editor.dragging = null
                    if (done)
                        editor.committed(done)
                }
            }
        }
    }
}
