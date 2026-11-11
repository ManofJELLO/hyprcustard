import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import dev.hyprcustard.gui

// Layer surfaces: bars, launchers, notifications, wallpapers.
//
// Layers carry no rule tags, so these are plugin calls made at session start
// rather than window rules.
Item {
    id: page

    function updateLayer(layer, field, shaderRef) {
        var copy = JSON.parse(JSON.stringify(layer))
        copy[field] = shaderRef
        App.run("layer.update", { layer: copy })
    }

    function setEnabled(layer, on) {
        var copy = JSON.parse(JSON.stringify(layer))
        copy.enabled = on
        App.run("layer.update", { layer: copy })
    }

    ScrollView {
        id: scroll
        anchors.fill: parent
        clip: true
        contentWidth: availableWidth

        ColumnLayout {
            width: Math.min(scroll.availableWidth, Theme.contentMax)
            x: Math.round((scroll.availableWidth - width) / 2)
            spacing: Theme.pad

            // --- adding ---
            Card {
                Layout.fillWidth: true
                title: "Layer namespaces"
                subtitle: "A shader set on \"*\" applies to every layer and the per-namespace " +
                          "ones composite on top of it. That includes your wallpaper and your bar, " +
                          "so name namespaces individually if that is not what you want."

                RowLayout {
                    width: parent.width
                    spacing: Theme.gapSmall

                    Dropdown {
                        id: nsPicker
                        Layout.fillWidth: true
                        model: {
                            var out = ["*  (every layer)"]
                            for (var i = 0; i < App.liveNamespaces.length; ++i)
                                out.push(App.liveNamespaces[i])
                            return out
                        }
                        currentIndex: 0
                    }

                    PillButton {
                        text: "Add"
                        primary: true
                        onClicked: {
                            var ns = nsPicker.currentIndex === 0
                                     ? "*" : App.liveNamespaces[nsPicker.currentIndex - 1]
                            App.run("layer.add", { namespace: ns })
                        }
                    }
                }

                RowLayout {
                    width: parent.width
                    spacing: Theme.gapSmall

                    LineEdit {
                        id: customNs
                        Layout.fillWidth: true
                        mono: true
                        placeholderText: "or type a namespace — rofi, waybar, mako…"
                        onCommit: function (v) {
                            if (v.trim() !== "") {
                                App.run("layer.add", { namespace: v.trim() })
                                text = ""
                            }
                        }
                    }

                    PillButton {
                        text: "Add"
                        onClicked: {
                            if (customNs.text.trim() !== "") {
                                App.run("layer.add", { namespace: customNs.text.trim() })
                                customNs.text = ""
                            }
                        }
                    }
                }

                Text {
                    width: parent.width
                    visible: !App.hyprlandRunning
                    wrapMode: Text.WordWrap
                    color: Theme.muted
                    font.pixelSize: Theme.fontSizeSmall
                    text: "Hyprland is not running, so there are no live namespaces to offer. " +
                          "`hyprctl layers` lists them on a running session."
                }
            }

            EmptyState {
                Layout.fillWidth: true
                Layout.topMargin: Theme.pad * 2
                visible: App.layers.length === 0
                title: "No layers configured"
                body: "Add a namespace above to give a bar, launcher or wallpaper its own shader."
            }

            // --- one card per namespace ---
            Repeater {
                model: App.layers

                Card {
                    id: layerCard

                    required property var modelData

                    Layout.fillWidth: true
                    title: layerCard.modelData.namespace === "*"
                           ? "*  — every layer"
                           : layerCard.modelData.namespace

                    RowLayout {
                        width: parent.width
                        spacing: Theme.gap

                        Toggle {
                            text: "enabled"
                            checked: layerCard.modelData.enabled
                            onToggled: page.setEnabled(layerCard.modelData, checked)
                        }

                        Item {
                            Layout.fillWidth: true
                        }

                        PillButton {
                            text: "Remove"
                            danger: true
                            onClicked: App.run("layer.remove", { id: layerCard.modelData.id })
                        }
                    }

                    FieldRow {
                        width: parent.width
                        label: "Shader"
                        hint: "layershader — always on this layer"

                        ShaderPicker {
                            width: parent.width
                            path: layerCard.modelData.shader ? layerCard.modelData.shader.path : ""
                            onPicked: function (path, duration, isDefault) {
                                page.updateLayer(layerCard.modelData, "shader",
                                                 path === "" ? null : { path: path })
                            }
                        }
                    }

                    FieldRow {
                        width: parent.width
                        label: "Open animation"
                        hint: "layeropenanim — plays as the layer appears"

                        ShaderPicker {
                            width: parent.width
                            showDuration: true
                            path: layerCard.modelData.open_anim
                                  ? layerCard.modelData.open_anim.path : ""
                            duration: layerCard.modelData.open_anim
                                      && layerCard.modelData.open_anim.duration !== undefined
                                      ? layerCard.modelData.open_anim.duration : null
                            onPicked: function (path, duration, isDefault) {
                                page.updateLayer(layerCard.modelData, "open_anim",
                                                 path === ""
                                                 ? null
                                                 : { path: path, duration: duration })
                            }
                        }
                    }

                    FieldRow {
                        width: parent.width
                        label: "Close animation"
                        hint: "layercloseanim — must end fully transparent"

                        ShaderPicker {
                            width: parent.width
                            showDuration: true
                            path: layerCard.modelData.close_anim
                                  ? layerCard.modelData.close_anim.path : ""
                            duration: layerCard.modelData.close_anim
                                      && layerCard.modelData.close_anim.duration !== undefined
                                      ? layerCard.modelData.close_anim.duration : null
                            onPicked: function (path, duration, isDefault) {
                                page.updateLayer(layerCard.modelData, "close_anim",
                                                 path === ""
                                                 ? null
                                                 : { path: path, duration: duration })
                            }
                        }
                    }

                    // Motion-driven shaders are a silent no-op on a layer, which
                    // is the single most confusing thing about layer shaders.
                    Rectangle {
                        width: parent.width
                        height: motionWarn.implicitHeight + Theme.gap * 2
                        radius: Theme.radiusSmall
                        color: Theme.wash(Theme.warn, 0.14)
                        border.width: 1
                        border.color: Theme.wash(Theme.warn, 0.5)
                        visible: {
                            var fields = [layerCard.modelData.shader,
                                          layerCard.modelData.open_anim,
                                          layerCard.modelData.close_anim]
                            for (var i = 0; i < fields.length; ++i) {
                                if (!fields[i])
                                    continue
                                var s = App.shaderByPath(fields[i].path)
                                if (s && s.isMotionDriven)
                                    return true
                            }
                            return false
                        }

                        Text {
                            id: motionWarn
                            anchors.fill: parent
                            anchors.margins: Theme.gap
                            wrapMode: Text.WordWrap
                            color: Theme.warn
                            font.pixelSize: Theme.fontSizeSmall
                            text: "One of these shaders scales its effect by velocity. A layer " +
                                  "never reports motion, so every motion uniform reads zero and " +
                                  "the shader renders the surface untouched — with no error. " +
                                  "Drive it from progress instead."
                        }
                    }
                }
            }

            Item {
                Layout.preferredHeight: Theme.pad
            }
        }
    }
}
