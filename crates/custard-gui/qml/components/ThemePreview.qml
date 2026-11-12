import QtQuick
import dev.hyprcustard.gui

// Two windows drawn the way a theme would draw them: gaps, rounding, border
// gradients, shadow, glow and the unfocused window's opacity. A sketch rather
// than a screenshot, so a list of themes can be compared at a glance.
Item {
    id: sketch

    // A theme's `options`, as the backend hands them out.
    property var options: ({})

    implicitWidth: 260
    implicitHeight: 150

    function opt(key, fallback) {
        var v = sketch.options[key]
        return v === undefined || v === null ? fallback : v
    }

    // `#aarrggbb` or `rgba(rrggbbaa)` as a CSS color the canvas takes.
    function css(c) {
        var s = String(c || "#ff000000").trim().toLowerCase()
        var a, r, g, b
        if (s.indexOf("rgba(") === 0) {
            var h = s.slice(5, -1)
            r = h.substr(0, 2); g = h.substr(2, 2); b = h.substr(4, 2); a = h.substr(6, 2) || "ff"
        } else if (s.indexOf("rgb(") === 0) {
            var h2 = s.slice(4, -1)
            r = h2.substr(0, 2); g = h2.substr(2, 2); b = h2.substr(4, 2); a = "ff"
        } else {
            var h3 = s.replace("#", "")
            if (h3.length === 6)
                h3 = "ff" + h3
            a = h3.substr(0, 2); r = h3.substr(2, 2); g = h3.substr(4, 2); b = h3.substr(6, 2)
        }
        return "rgba(" + parseInt(r, 16) + "," + parseInt(g, 16) + "," + parseInt(b, 16) + ","
            + (parseInt(a, 16) / 255).toFixed(3) + ")"
    }

    onOptionsChanged: art.requestPaint()
    onWidthChanged: art.requestPaint()
    onHeightChanged: art.requestPaint()

    Canvas {
        id: art
        anchors.fill: parent

        function roundRect(ctx, x, y, w, h, r) {
            r = Math.max(0, Math.min(r, w / 2, h / 2))
            ctx.beginPath()
            ctx.moveTo(x + r, y)
            ctx.arcTo(x + w, y, x + w, y + h, r)
            ctx.arcTo(x + w, y + h, x, y + h, r)
            ctx.arcTo(x, y + h, x, y, r)
            ctx.arcTo(x, y, x + w, y, r)
            ctx.closePath()
        }

        function borderStyle(ctx, value, x, y, w, h) {
            var colors = (value && value.colors) || ["#ff888888"]
            if (colors.length === 1)
                return sketch.css(colors[0])
            // Hyprland measures the angle from left to right, turning clockwise.
            var a = ((value.angle || 0) * Math.PI) / 180
            var cx = x + w / 2, cy = y + h / 2
            var dx = Math.cos(a) * w / 2, dy = Math.sin(a) * h / 2
            var grad = ctx.createLinearGradient(cx - dx, cy - dy, cx + dx, cy + dy)
            for (var i = 0; i < colors.length; ++i)
                grad.addColorStop(i / (colors.length - 1), sketch.css(colors[i]))
            return grad
        }

        function drawWindow(ctx, x, y, w, h, focused) {
            var scale = 0.6
            var radius = sketch.opt("decoration.rounding", 0) * scale
            var bw = Math.max(0, sketch.opt("general.border_size", 1)) * scale
            var opacity = focused ? sketch.opt("decoration.active_opacity", 1)
                                  : sketch.opt("decoration.inactive_opacity", 1)

            // Shadow, then glow, both cast by the window's shape. The shape
            // itself is drawn far off to the side and only its shadow brought
            // back, so a translucent window does not show a dark fill through.
            var away = 10000
            ctx.save()
            if (sketch.opt("decoration.shadow.enabled", true)) {
                var off = sketch.opt("decoration.shadow.offset", [0, 0])
                ctx.shadowColor = sketch.css(focused ? sketch.opt("decoration.shadow.color", "#ee1a1a1a")
                                                     : sketch.opt("decoration.shadow.color_inactive",
                                                                  sketch.opt("decoration.shadow.color", "#ee1a1a1a")))
                ctx.shadowBlur = sketch.opt("decoration.shadow.range", 4) * scale
                ctx.shadowOffsetX = off[0] * scale - away
                ctx.shadowOffsetY = off[1] * scale
                roundRect(ctx, x + away, y, w, h, radius)
                ctx.fillStyle = "rgba(0,0,0,1)"
                ctx.fill()
            }
            if (sketch.opt("decoration.glow.enabled", false)) {
                ctx.shadowColor = sketch.css(sketch.opt("decoration.glow.color", "#ee33ccff"))
                ctx.shadowBlur = sketch.opt("decoration.glow.range", 10) * scale
                ctx.shadowOffsetX = -away
                ctx.shadowOffsetY = 0
                roundRect(ctx, x + away, y, w, h, radius)
                ctx.fillStyle = "rgba(0,0,0,1)"
                ctx.fill()
            }
            ctx.restore()

            ctx.save()
            ctx.globalAlpha = opacity
            // Border: the whole shape in the border color, the window inside it.
            if (bw > 0) {
                roundRect(ctx, x, y, w, h, radius)
                ctx.fillStyle = borderStyle(ctx, sketch.opt(focused ? "general.col.active_border"
                                                                    : "general.col.inactive_border"),
                                            x, y, w, h)
                ctx.fill()
            }
            roundRect(ctx, x + bw, y + bw, w - 2 * bw, h - 2 * bw, Math.max(0, radius - bw))
            ctx.fillStyle = focused ? "#2b2b33" : "#24242a"
            ctx.fill()
            // A title line, so it reads as a window.
            ctx.fillStyle = focused ? "rgba(255,255,255,0.35)" : "rgba(255,255,255,0.18)"
            ctx.fillRect(x + bw + 10, y + bw + 10, (w - 2 * bw) * 0.45, 4)
            ctx.fillRect(x + bw + 10, y + bw + 20, (w - 2 * bw) * 0.3, 4)
            ctx.restore()
        }

        onPaint: {
            var ctx = getContext("2d")
            ctx.reset()

            // A neutral wallpaper.
            var bg = ctx.createLinearGradient(0, 0, width, height)
            bg.addColorStop(0, "#3d4a5c")
            bg.addColorStop(1, "#5c4a5a")
            ctx.fillStyle = bg
            ctx.fillRect(0, 0, width, height)

            var scale = 0.6
            var outer = sketch.opt("general.gaps_out", [20, 20, 20, 20])
            var inner = sketch.opt("general.gaps_in", [5, 5, 5, 5])
            var top = Math.max(4, outer[0] * scale), right = Math.max(4, outer[1] * scale)
            var bottom = Math.max(4, outer[2] * scale), left = Math.max(4, outer[3] * scale)
            var between = Math.max(2, (inner[1] + inner[3]) * scale)
            var w = (width - left - right - between) / 2
            var h = height - top - bottom

            drawWindow(ctx, left + w + between, top, w, h, false)
            drawWindow(ctx, left, top, w, h, true)
        }
    }
}
