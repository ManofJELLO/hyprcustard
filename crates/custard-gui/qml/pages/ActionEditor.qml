import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import dev.hyprcustard.gui

// Edits one plugin call — the thing a keybind runs, or a startup action does.
Item {
    id: editor

    property var action: null

    signal edited(var action)

    readonly property var kinds: [
        { id: "toggle_window_shader", label: "Toggle on the focused window",
          wants: "shader" },
        { id: "toggle_class_shader",  label: "Toggle on every window of a class",
          wants: "class+shader" },
        { id: "class_shader",         label: "Force on every window of a class",
          wants: "class+shader?" },
        { id: "toggle_layer_shader",  label: "Toggle on a layer namespace",
          wants: "namespace+shader" },
        { id: "layer_shader",         label: "Force on a layer namespace",
          wants: "namespace+shader?" },
        { id: "reload_shaders",       label: "Reload all shaders", wants: "" }
    ]

    // A call picked but not yet complete. A toggle has nothing to toggle
    // without a shader, so the config cannot hold one — and picking the kind
    // before the shader is the obvious order to work in. The half-built call
    // is therefore kept here, on screen and editable, rather than sent to the
    // engine to be refused.
    property var draft: null

    readonly property var shown: editor.draft ? editor.draft
                                 : (editor.action ? editor.action
                                                  : ({ action: "reload_shaders" }))

    readonly property string kindId: editor.shown.action ? editor.shown.action
                                                         : "reload_shaders"
    readonly property var kind: editor.kindOf(editor.kindId)

    readonly property bool wantsClass: editor.kind.wants.indexOf("class") >= 0
    readonly property bool wantsNamespace: editor.kind.wants.indexOf("namespace") >= 0
    readonly property bool wantsShader: editor.kind.wants.indexOf("shader") >= 0
    readonly property bool shaderOptional: editor.kind.wants.indexOf("shader?") >= 0

    implicitHeight: column.implicitHeight

    function kindOf(id) {
        for (var i = 0; i < editor.kinds.length; ++i)
            if (editor.kinds[i].id === id)
                return editor.kinds[i]
        return editor.kinds[editor.kinds.length - 1]
    }

    function currentClass() {
        return editor.shown["class"] ? editor.shown["class"] : ""
    }

    function currentNamespace() {
        return editor.shown.namespace ? editor.shown.namespace : ""
    }

    function currentShaderPath() {
        if (editor.shown.shader && editor.shown.shader.path)
            return editor.shown.shader.path
        return ""
    }

    // Build the call for `kindId` out of what is on screen, plus `overrides`,
    // and either send it or hold it until it is whole.
    function apply(kindId, overrides) {
        var wants = editor.kindOf(kindId).wants
        var next = { action: kindId }
        if (wants.indexOf("class") >= 0)
            next["class"] = editor.currentClass()
        if (wants.indexOf("namespace") >= 0)
            next.namespace = editor.currentNamespace()
        if (wants.indexOf("shader") >= 0) {
            var p = editor.currentShaderPath()
            next.shader = p === "" ? null : { path: p }
        }
        for (var key in overrides)
            next[key] = overrides[key]
        if (wants.indexOf("shader") >= 0 && wants.indexOf("shader?") < 0 && !next.shader) {
            editor.draft = next
            return
        }
        editor.draft = null
        editor.edited(next)
    }

    function emitWith(overrides) {
        editor.apply(editor.kindId, overrides)
    }

    Column {
        id: column
        width: parent.width
        spacing: Theme.gapSmall

        Dropdown {
            width: parent.width
            model: editor.kinds
            textRole: "label"
            currentIndex: {
                for (var i = 0; i < editor.kinds.length; ++i)
                    if (editor.kinds[i].id === editor.kindId)
                        return i
                return 0
            }
            onActivated: function (index) {
                editor.apply(editor.kinds[index].id, ({}))
            }
        }

        RowLayout {
            width: parent.width
            visible: editor.wantsClass
            spacing: Theme.gapSmall

            LineEdit {
                Layout.fillWidth: true
                mono: true
                placeholderText: "window class"
                text: editor.currentClass()
                onCommit: function (v) {
                    editor.emitWith({ "class": v.trim() })
                }
            }

            Dropdown {
                Layout.preferredWidth: 150
                visible: App.liveClasses.length > 0
                model: ["open windows…"].concat(App.liveClasses)
                currentIndex: 0
                onActivated: function (index) {
                    if (index === 0)
                        return
                    editor.emitWith({ "class": App.liveClasses[index - 1] })
                    currentIndex = 0
                }
            }
        }

        RowLayout {
            width: parent.width
            visible: editor.wantsNamespace
            spacing: Theme.gapSmall

            LineEdit {
                Layout.fillWidth: true
                mono: true
                placeholderText: "layer namespace"
                text: editor.currentNamespace()
                onCommit: function (v) {
                    editor.emitWith({ namespace: v.trim() })
                }
            }

            Dropdown {
                Layout.preferredWidth: 150
                visible: App.liveNamespaces.length > 0
                model: ["live layers…"].concat(App.liveNamespaces)
                currentIndex: 0
                onActivated: function (index) {
                    if (index === 0)
                        return
                    editor.emitWith({ namespace: App.liveNamespaces[index - 1] })
                    currentIndex = 0
                }
            }
        }

        ShaderPicker {
            width: parent.width
            visible: editor.wantsShader
            path: editor.currentShaderPath()
            placeholder: editor.shaderOptional ? "clear the shader" : "pick a shader"
            onPicked: function (path, duration, isDefault) {
                editor.emitWith({ shader: path === "" ? null : { path: path } })
            }
        }

        Text {
            width: parent.width
            visible: editor.draft !== null
            text: "Pick a shader to finish this action — until then it is not saved."
            color: Theme.warn
            font.pixelSize: Theme.fontSizeSmall
            wrapMode: Text.WordWrap
        }
    }
}
