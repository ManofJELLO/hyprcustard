import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import dev.hyprcustard.gui

// One row of the Look page: the control that fits the option's kind, a
// marker when it is set here, and a button to stop setting it.
//
// The row is handed a key, not an option, and looks the option up on the
// page each time the state changes. That keeps the row alive through a state
// push, so a slider is not destroyed while someone is dragging it.
Item {
    id: row

    // The Look page, for its option table and its live-edit helpers.
    property var host: null
    property string key: ""
    // Set by a rule's editor: the row's button then takes the effect off the
    // rule instead of resetting a Look option.
    property var removeFn: null
    readonly property var opt: (row.host && row.host.byKey[row.key]) || ({ label: "", kind: "", value: null })
    readonly property string parentKey: row.host ? row.host.parentOf(row.key) : ""
    readonly property bool switchedOn: row.parentKey === ""
                                       || ((row.host && row.host.byKey[row.parentKey]) || {}).value === true

    width: parent ? parent.width : 0
    implicitHeight: body.implicitHeight + (hintText.visible ? hintText.implicitHeight + 3 : 0)
    opacity: row.switchedOn ? 1 : 0.45

    // Set here: accent once saved, warning color while unsaved.
    Rectangle {
        visible: row.opt.overridden === true
        x: -Theme.pad + 4
        width: 3
        height: body.height
        radius: 1.5
        color: row.opt.changed === true ? Theme.warn : Theme.accent
    }

    Loader {
        id: body
        anchors.left: parent.left
        anchors.right: resetButton.left
        anchors.rightMargin: Theme.gap
        sourceComponent: {
            switch (row.opt.kind) {
            case "int":
            case "float":
                return numberControl
            case "bool":
                return boolControl
            case "color":
                return colorControl
            case "gradient":
                return gradientControl
            case "sides":
                return sidesControl
            case "vec2":
                return vecControl
            case "text":
                return textControl
            }
            return null
        }
    }

    PillButton {
        id: resetButton
        anchors.right: parent.right
        anchors.top: parent.top
        compact: true
        text: row.removeFn ? "Remove" : "Reset"
        enabled: row.opt.overridden === true
        opacity: enabled ? 1 : 0
        tooltip: row.removeFn ? "Take this off the rule."
                              : "Stop setting this here. Your own config's value applies again "
                                + "once you save."
        onClicked: {
            if (row.removeFn)
                row.removeFn(row.key)
            else
                App.run("look.reset", { key: row.key })
        }
    }

    Text {
        id: hintText
        anchors.top: body.bottom
        anchors.topMargin: 3
        width: body.width
        text: row.opt.hint || ""
        visible: text !== ""
        wrapMode: Text.WordWrap
        color: Theme.muted
        font.pixelSize: Theme.fontSizeSmall
    }

    Component {
        id: numberControl

        LabeledSlider {
            id: slider
            label: row.opt.label
            from: row.opt.min
            to: row.opt.max
            stepSize: row.opt.step
            integer: row.opt.kind === "int"
            enabled: row.switchedOn
            Component.onCompleted: slider.value = row.opt.value
            Connections {
                target: row
                function onOptChanged() {
                    if (!slider.pressed)
                        slider.value = row.opt.value
                }
            }
            onMoved: function (v) { row.host.tryLive(row.key, v) }
            onSettled: function (v) { row.host.set(row.key, v) }
        }
    }

    Component {
        id: boolControl

        Toggle {
            id: toggle
            text: row.opt.label
            enabled: row.switchedOn
            checked: row.opt.value === true
            onToggled: {
                var v = toggle.checked
                toggle.checked = Qt.binding(function () { return row.opt.value === true })
                row.host.set(row.key, v)
            }
        }
    }

    Component {
        id: colorControl

        RowLayout {
            spacing: Theme.gap
            enabled: row.switchedOn

            Text {
                Layout.preferredWidth: 150
                text: row.opt.label
                color: Theme.fgDim
                font.pixelSize: Theme.fontSize
            }
            ColorButton {
                value: row.opt.value || "#ff000000"
                tooltip: "Pick a color"
                onClicked: row.host.pickColor(row.opt.value, function (c, done) {
                    if (done)
                        row.host.set(row.key, c)
                    else
                        row.host.tryLive(row.key, c)
                })
            }
            Text {
                // Unset, and so wearing the focused color.
                visible: row.opt.following === true
                text: "same as " + (((row.host && row.host.byKey[row.opt.follows]) || {}).label
                                    || "its pair").toLowerCase() + " until set"
                color: Theme.muted
                font.pixelSize: Theme.fontSizeSmall
                font.italic: true
            }
            LineEdit {
                id: hex
                Layout.preferredWidth: 120
                mono: true
                text: row.host.hyprHex(row.opt.value)
                onCommit: function (v) {
                    hex.text = Qt.binding(function () { return row.host.hyprHex(row.opt.value) })
                    if (v.trim() !== row.host.hyprHex(row.opt.value))
                        row.host.set(row.key, v.trim())
                }
            }
            Item {
                Layout.fillWidth: true
            }
        }
    }

    Component {
        id: gradientControl

        Column {
            id: ramp
            spacing: Theme.gapSmall

            readonly property var colors: (row.opt.value && row.opt.value.colors) || []
            readonly property real angle: (row.opt.value && row.opt.value.angle) || 0

            function withColor(index, c) {
                var list = ramp.colors.slice()
                list[index] = c
                return { colors: list, angle: ramp.angle }
            }

            RowLayout {
                width: parent.width
                spacing: Theme.gapSmall

                Text {
                    Layout.preferredWidth: 150 + Theme.gap - Theme.gapSmall
                    text: row.opt.label
                    color: Theme.fgDim
                    font.pixelSize: Theme.fontSize
                }
                Repeater {
                    model: ramp.colors.length

                    ColorButton {
                        required property int index
                        value: ramp.colors[index] || "#ff000000"
                        tooltip: "Color " + (index + 1) + " — click to change"
                        onClicked: {
                            var at = index
                            row.host.pickColor(ramp.colors[at], function (c, done) {
                                var v = ramp.withColor(at, c)
                                if (done)
                                    row.host.set(row.key, v)
                                else
                                    row.host.tryLive(row.key, v)
                            })
                        }
                    }
                }
                PillButton {
                    compact: true
                    text: "+"
                    enabled: ramp.colors.length < 10
                    tooltip: "Add a color to the gradient"
                    onClicked: row.host.set(row.key, {
                        colors: ramp.colors.concat([ramp.colors[ramp.colors.length - 1]]),
                        angle: ramp.angle
                    })
                }
                PillButton {
                    compact: true
                    text: "−"
                    enabled: ramp.colors.length > 1
                    tooltip: "Take the last color off"
                    onClicked: row.host.set(row.key, {
                        colors: ramp.colors.slice(0, ramp.colors.length - 1),
                        angle: ramp.angle
                    })
                }
                Item {
                    Layout.fillWidth: true
                }
            }

            LabeledSlider {
                id: angleSlider
                width: parent.width
                visible: ramp.colors.length > 1
                label: "Angle"
                from: 0
                to: 359
                stepSize: 1
                integer: true
                Component.onCompleted: angleSlider.value = ramp.angle
                Connections {
                    target: row
                    function onOptChanged() {
                        if (!angleSlider.pressed)
                            angleSlider.value = ramp.angle
                    }
                }
                onMoved: function (v) {
                    row.host.tryLive(row.key, { colors: ramp.colors, angle: v })
                }
                onSettled: function (v) {
                    row.host.set(row.key, { colors: ramp.colors, angle: v })
                }
            }

            LineEdit {
                id: gradientEdit
                width: parent.width
                mono: true
                placeholderText: "33ccffee 00ff99ee 45deg"
                text: row.host.gradientText(row.opt.value)
                onCommit: function (v) {
                    gradientEdit.text = Qt.binding(function () {
                        return row.host.gradientText(row.opt.value)
                    })
                    if (v.trim() !== row.host.gradientText(row.opt.value))
                        row.host.set(row.key, row.host.parseGradient(v))
                }
            }
        }
    }

    Component {
        id: sidesControl

        Column {
            id: sides
            spacing: Theme.gapSmall

            readonly property var values: row.opt.value || [0, 0, 0, 0]
            readonly property bool uniform: sides.values[0] === sides.values[1]
                                            && sides.values[0] === sides.values[2]
                                            && sides.values[0] === sides.values[3]

            LabeledSlider {
                id: all
                width: parent.width
                label: row.opt.label + (sides.uniform ? "" : "  (sets every side)")
                from: row.opt.min
                to: row.opt.max
                stepSize: 1
                integer: true
                Component.onCompleted: all.value = Math.max.apply(null, sides.values)
                Connections {
                    target: row
                    function onOptChanged() {
                        if (!all.pressed)
                            all.value = Math.max.apply(null, sides.values)
                    }
                }
                onMoved: function (v) { row.host.tryLive(row.key, [v]) }
                onSettled: function (v) { row.host.set(row.key, [v, v, v, v]) }
            }

            RowLayout {
                width: parent.width
                spacing: Theme.gap

                Text {
                    text: "Per side — top, right, bottom, left"
                    color: Theme.muted
                    font.pixelSize: Theme.fontSizeSmall
                }
                LineEdit {
                    id: perSide
                    Layout.preferredWidth: 130
                    implicitHeight: 26
                    mono: true
                    text: sides.values.join(" ")
                    onCommit: function (v) {
                        perSide.text = Qt.binding(function () { return sides.values.join(" ") })
                        var parts = v.trim().split(/[\s,]+/).map(function (s) { return parseInt(s, 10) })
                        if (parts.some(isNaN) || v.trim() === sides.values.join(" "))
                            return
                        row.host.set(row.key, parts)
                    }
                }
                Item {
                    Layout.fillWidth: true
                }
            }
        }
    }

    Component {
        id: vecControl

        Column {
            id: vec
            spacing: 2
            enabled: row.switchedOn

            readonly property var xy: row.opt.value || [0, 0]

            Text {
                text: row.opt.label
                color: Theme.fgDim
                font.pixelSize: Theme.fontSize
            }

            RowLayout {
                width: parent.width
                spacing: Theme.pad

                Repeater {
                    model: 2

                    LabeledSlider {
                        id: axis
                        required property int index
                        Layout.fillWidth: true
                        label: (row.opt.parts && row.opt.parts.length === 2)
                               ? row.opt.parts[axis.index]
                               : (axis.index === 0 ? "Across" : "Down")
                        from: row.opt.min
                        to: row.opt.max
                        stepSize: row.opt.step
                        integer: row.opt.step >= 1
                        Component.onCompleted: axis.value = vec.xy[axis.index]
                        Connections {
                            target: row
                            function onOptChanged() {
                                if (!axis.pressed)
                                    axis.value = vec.xy[axis.index]
                            }
                        }
                        function pair(v) {
                            var p = vec.xy.slice()
                            p[axis.index] = v
                            return p
                        }
                        onMoved: function (v) { row.host.tryLive(row.key, axis.pair(v)) }
                        onSettled: function (v) { row.host.set(row.key, axis.pair(v)) }
                    }
                }
            }
        }
    }

    Component {
        id: textControl

        RowLayout {
            spacing: Theme.gap
            enabled: row.switchedOn

            Text {
                Layout.preferredWidth: 150
                text: row.opt.label
                color: Theme.fgDim
                font.pixelSize: Theme.fontSize
            }
            LineEdit {
                id: line
                Layout.fillWidth: true
                mono: true
                text: row.opt.value || ""
                onCommit: function (v) {
                    line.text = Qt.binding(function () { return row.opt.value || "" })
                    if (v.trim() !== "" && v.trim() !== row.opt.value)
                        row.host.set(row.key, v.trim())
                }
            }
        }
    }
}
