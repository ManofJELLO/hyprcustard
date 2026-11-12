import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import dev.hyprcustard.gui

// One tunable constant from a shader: a slider, a switch, a color, or a small
// group of sliders, depending on what the type and the annotations say.
//
// An edit is staged, not written: `value` is what is being edited and
// `savedValue` is what the file still says, so this row can show that it has
// moved and offer to put it back. The file changes when the shader is saved.
Item {
    id: editor

    property var param: null
    property string shaderPath: ""

    readonly property var kind: param && param.kind ? param.kind : ({})
    readonly property string kindName: kind.kind || "scalar"
    readonly property bool annotated: param ? param.annotated === true : false
    readonly property bool staged: param ? param.staged === true : false

    // What the file still says, formatted the way the editor shows values.
    readonly property string savedText: {
        if (!editor.param || editor.param.savedValue === undefined)
            return ""
        if (editor.kindName === "bool")
            return editor.param.savedValue ? "on" : "off"
        return editor.savedComponents().map(function (c) {
            return Math.round(c * 1000) / 1000
        }).join(", ")
    }

    implicitHeight: column.implicitHeight

    function send(value) {
        if (!editor.param || editor.shaderPath === "")
            return
        App.run("shader.setParam", {
            path: editor.shaderPath,
            name: editor.param.name,
            value: value
        })
    }

    function revert() {
        if (!editor.param || editor.shaderPath === "")
            return
        App.run("shader.revert", { path: editor.shaderPath, item: editor.param.name })
    }

    // A parameter's value as a plain JS array, whatever shape it arrives in.
    //
    // A vec2/vec3/vec4 is a JSON array in the state document, but by the time
    // it reaches a delegate the Repeater's model has converted it to a
    // QVariantList — for which `Array.isArray` reads back false. Taking that
    // answer at its word wraps the whole vector in an array, so component 0
    // becomes the list itself (NaN in a slider) and the rest read as zero.
    // Dropdown.qml carries the same warning about the same conversion.
    function toComponents(v) {
        if (v === undefined || v === null)
            return []
        if (typeof v === "number")
            return [v]
        if (typeof v === "boolean")
            return [v ? 1 : 0]
        if (v.length === undefined)
            return [v]
        var out = []
        for (var i = 0; i < v.length; ++i)
            out.push(v[i])
        return out
    }

    // The file's own value, component-wise, for the tick on each track.
    function savedComponents() {
        if (!editor.param)
            return []
        return editor.toComponents(editor.param.savedValue)
    }

    // The tick only earns its place while the value has actually moved.
    function mark(index) {
        if (!editor.staged)
            return null
        var c = editor.savedComponents()
        return c[index] === undefined ? null : c[index]
    }

    function components() {
        if (!editor.param)
            return []
        return editor.toComponents(editor.param.value)
    }

    function withComponent(index, value) {
        var out = editor.components()
        out[index] = value
        return out
    }

    Column {
        id: column
        width: parent.width
        spacing: 4

        // Header: name, GLSL type, and whether the range was declared or guessed.
        RowLayout {
            width: parent.width
            spacing: Theme.gapSmall

            Text {
                text: editor.param ? editor.param.label : ""
                color: Theme.fg
                font.pixelSize: Theme.fontSize
                font.weight: Font.DemiBold
            }

            Text {
                text: editor.param ? editor.param.name : ""
                color: Theme.muted
                font.family: Theme.monoFamily
                font.pixelSize: Theme.fontSizeSmall
            }

            Item {
                Layout.fillWidth: true
            }

            Badge {
                visible: editor.staged
                text: "was " + editor.savedText
                tint: Theme.warn

                HoverHandler {
                    id: stagedHover
                }
                Tip {
                    visible: stagedHover.hovered
                    delay: 400
                    text: "Staged, not written. The file still says " + editor.savedText
                          + ", which is where the tick on the slider is."
                }
            }

            PillButton {
                visible: editor.staged
                compact: true
                text: "Revert"
                tooltip: "Put this one value back to " + editor.savedText
                onClicked: editor.revert()
            }

            Badge {
                text: editor.param ? editor.param.glsl_type : ""
                tint: Theme.muted
            }

            Badge {
                visible: !editor.annotated && editor.kindName !== "bool"
                text: "guessed range"
                tint: Theme.warn

                HoverHandler {
                    id: guessHover
                }
                Tip {
                    visible: guessHover.hovered
                    delay: 400
                    text: "No // @param annotation, so the range was inferred from the " +
                          "current value. Add one to set a real range and label."
                }
            }
        }

        // --- bool ---
        Toggle {
            visible: editor.kindName === "bool"
            checked: editor.param ? editor.param.value !== 0 : false
            onToggled: editor.send(checked)
        }

        // --- scalar ---
        LabeledSlider {
            width: parent.width
            visible: editor.kindName === "scalar"
            label: ""
            from: editor.kind.min !== undefined ? editor.kind.min : 0
            to: editor.kind.max !== undefined ? editor.kind.max : 1
            stepSize: editor.kind.step !== undefined ? editor.kind.step : 0.01
            integer: editor.kind.integer === true
            value: {
                var c = editor.components()
                return c[0] === undefined ? 0 : c[0]
            }
            markValue: editor.mark(0)
            onSettled: function (v) {
                editor.send(editor.kind.integer === true ? Math.round(v) : v)
            }
        }

        // --- color ---
        Row {
            width: parent.width
            visible: editor.kindName === "color"
            spacing: Theme.gap

            // The color as it stands, and — while it has moved — the one the
            // file still holds underneath it. Three numbers in a badge are not
            // a color; this is.
            Column {
                id: swatch
                width: 56
                spacing: 3

                Rectangle {
                    width: parent.width
                    height: editor.staged ? 40 : 56
                    radius: Theme.radiusSmall
                    border.width: 1
                    border.color: Theme.border
                    color: {
                        var c = editor.components()
                        return Qt.rgba(c[0] || 0, c[1] || 0, c[2] || 0,
                                       editor.kind.alpha === true ? (c[3] === undefined ? 1 : c[3]) : 1)
                    }

                    Behavior on height {
                        NumberAnimation { duration: 90 }
                    }
                }

                Rectangle {
                    width: parent.width
                    height: 13
                    visible: editor.staged
                    radius: Theme.radiusSmall
                    border.width: 1
                    border.color: Theme.border
                    color: {
                        var c = editor.savedComponents()
                        return Qt.rgba(c[0] || 0, c[1] || 0, c[2] || 0,
                                       editor.kind.alpha === true ? (c[3] === undefined ? 1 : c[3]) : 1)
                    }

                    HoverHandler {
                        id: savedSwatchHover
                    }
                    Tip {
                        visible: savedSwatchHover.hovered
                        delay: 400
                        text: "The color in the file: " + editor.savedText
                    }
                }
            }

            Column {
                width: parent.width - swatch.width - Theme.gap
                spacing: 2

                Repeater {
                    model: editor.kind.alpha === true
                           ? ["red", "green", "blue", "alpha"]
                           : ["red", "green", "blue"]

                    LabeledSlider {
                        required property string modelData
                        required property int index

                        width: parent.width
                        label: modelData
                        from: 0
                        to: 1
                        stepSize: 0.01
                        value: {
                            var c = editor.components()
                            return c[index] === undefined ? 0 : c[index]
                        }
                        markValue: editor.mark(index)
                        onSettled: function (v) {
                            editor.send(editor.withComponent(index, v))
                        }
                    }
                }
            }
        }

        // --- vector ---
        Column {
            width: parent.width
            visible: editor.kindName === "vector"
            spacing: 2

            Repeater {
                model: editor.kind.len !== undefined ? editor.kind.len : 0

                LabeledSlider {
                    required property int index

                    width: parent.width
                    label: ["x", "y", "z", "w"][index] || ("[" + index + "]")
                    from: editor.kind.min !== undefined ? editor.kind.min : 0
                    to: editor.kind.max !== undefined ? editor.kind.max : 1
                    stepSize: editor.kind.step !== undefined ? editor.kind.step : 0.01
                    value: {
                        var c = editor.components()
                        return c[index] === undefined ? 0 : c[index]
                    }
                    markValue: editor.mark(index)
                    onSettled: function (v) {
                        editor.send(editor.withComponent(index, v))
                    }
                }
            }
        }

        Rectangle {
            width: parent.width
            height: 1
            color: Theme.border
            opacity: 0.5
        }
    }
}
