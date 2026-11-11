import QtQuick
import QtQuick.Controls.Basic
import dev.hyprcustard.gui

// A stand-in window doing roughly what the animation does, on the same curve
// and over the same time. Hyprland's own drawing differs in detail; this is
// for feeling out a curve and a speed without opening windows.
Item {
    id: demo

    // "windows", "layers", "workspaces", "angle" or "plain".
    property string family: "plain"
    // As Hyprland writes it, e.g. "popin 80%", or empty for the default.
    property string styleText: ""
    // Tenths of a second.
    property real speed: 5
    property var points: [0, 0, 1, 1]
    property bool animate: true
    // Play again after a short rest.
    property bool repeat: true

    // 0 → 1 over the animation.
    property real progress: 1

    readonly property string base: demo.styleText.split(" ")[0] || demo.defaultStyle
    readonly property string param: demo.styleText.split(" ")[1] || ""
    readonly property string defaultStyle: {
        switch (demo.family) {
        case "windows":
            return "popin"
        case "layers":
            return "fade"
        case "workspaces":
            return "slide"
        case "angle":
            return "loop"
        }
        return "fade"
    }
    // A percentage parameter as a fraction, or the default.
    function percent(fallback) {
        var n = parseFloat(demo.param)
        return isNaN(n) ? fallback : Math.max(0, Math.min(100, n)) / 100
    }
    readonly property int duration: Math.max(1, Math.round(demo.speed * 100))

    implicitHeight: 180

    function play() {
        run.stop()
        rest.stop()
        if (!demo.animate) {
            demo.progress = 1
            return
        }
        run.duration = demo.duration
        run.easing.bezierCurve = [demo.points[0], demo.points[1], demo.points[2], demo.points[3], 1, 1]
        run.start()
    }

    onPointsChanged: demo.play()
    onSpeedChanged: demo.play()
    onStyleTextChanged: demo.play()
    onFamilyChanged: demo.play()
    onAnimateChanged: demo.play()
    Component.onCompleted: demo.play()

    NumberAnimation {
        id: run
        target: demo
        property: "progress"
        from: 0
        to: 1
        easing.type: Easing.BezierSpline
        onFinished: {
            if (demo.repeat)
                rest.start()
        }
    }
    Timer {
        id: rest
        interval: 700
        onTriggered: demo.play()
    }

    Rectangle {
        id: stage
        anchors.fill: parent
        radius: Theme.radiusSmall
        color: Theme.bgAlt
        border.width: 1
        border.color: Theme.border
        clip: true

        // The window being animated.
        Rectangle {
            id: pane

            readonly property real t: demo.progress
            readonly property real w: 140
            readonly property real h: 90
            readonly property real cx: (stage.width - pane.w) / 2
            readonly property real cy: (stage.height - pane.h) / 2

            width: pane.w
            height: pane.h
            radius: 8
            color: Theme.surfaceHi
            border.width: 2
            border.color: Theme.accent
            transformOrigin: Item.Center

            x: {
                var t = pane.t
                switch (demo.base) {
                case "slide":
                    if (demo.family === "workspaces")
                        return pane.cx + (1 - t) * stage.width
                    if (demo.param === "right")
                        return pane.cx + (1 - t) * (stage.width - pane.cx)
                    if (demo.param === "top" || demo.param === "bottom")
                        return pane.cx
                    return pane.cx - (1 - t) * (pane.cx + pane.w)
                case "slidefade":
                    return pane.cx + (1 - t) * stage.width * demo.percent(1)
                }
                return pane.cx
            }
            y: {
                var t = pane.t
                switch (demo.base) {
                case "slide":
                    if (demo.param === "top")
                        return pane.cy - (1 - t) * (pane.cy + pane.h)
                    if (demo.param === "bottom")
                        return pane.cy + (1 - t) * (stage.height - pane.cy)
                    return pane.cy
                case "slidevert":
                    return pane.cy + (1 - t) * stage.height
                case "slidefadevert":
                    return pane.cy + (1 - t) * stage.height * demo.percent(1)
                }
                return pane.cy
            }
            scale: {
                var t = pane.t
                if (demo.base === "popin") {
                    var from = demo.percent(0.8)
                    return from + (1 - from) * t
                }
                return 1
            }
            opacity: {
                switch (demo.base) {
                case "fade":
                case "popin":
                case "slidefade":
                case "slidefadevert":
                    return Math.max(0, Math.min(1, pane.t))
                }
                return 1
            }
            transform: Scale {
                origin.x: pane.w / 2
                origin.y: pane.h / 2
                yScale: demo.base === "gnomed" ? Math.max(0, pane.t) : 1
                xScale: demo.base === "gnomed" ? Math.max(0, 0.4 + 0.6 * pane.t) : 1
            }

            // A turning gradient border, for the angle leaves.
            Rectangle {
                visible: demo.family === "angle"
                anchors.centerIn: parent
                width: parent.width * 0.7
                height: 4
                radius: 2
                color: Theme.accent
                rotation: pane.t * 360
            }
        }

        Text {
            anchors.left: parent.left
            anchors.bottom: parent.bottom
            anchors.margins: Theme.gapSmall
            text: demo.animate
                  ? (demo.base + (demo.param !== "" ? " " + demo.param : "")
                     + (demo.styleText === "" ? " (default)" : "") + "  ·  " + demo.duration + " ms")
                  : "Not animated — it just appears"
            color: Theme.muted
            font.pixelSize: Theme.fontSizeSmall
        }
    }
}
