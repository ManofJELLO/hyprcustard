import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import dev.hyprcustard.gui

// Where things are, how the block is written, and what the app looks like.
Item {
    id: page

    function updateSettings(changes) {
        var copy = JSON.parse(JSON.stringify(App.settings))
        for (var k in changes)
            copy[k] = changes[k]
        App.run("settings.update", { settings: copy })
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

            // ---------------------------------------------------------------
            Card {
                Layout.fillWidth: true
                title: "Files"

                FieldRow {
                    width: parent.width
                    label: "Hyprland config"
                    hint: "The file the managed block is written into."

                    LineEdit {
                        width: parent.width
                        mono: true
                        text: App.settings.config_path || ""
                        onCommit: function (v) {
                            page.updateSettings({ config_path: v.trim() })
                        }
                    }
                }

                FieldRow {
                    width: parent.width
                    label: "Shader folder"
                    hint: "Scanned for .glsl files, one level of subfolders deep."

                    LineEdit {
                        width: parent.width
                        mono: true
                        text: App.state.shaderDir || ""
                        onCommit: function (v) {
                            App.run("config.update", { shaderDir: v.trim() })
                        }
                    }
                }

                FieldRow {
                    width: parent.width
                    label: "Backups to keep"
                    hint: "Per file, in ~/.config/hyprcustard/backups."

                    LineEdit {
                        width: 100
                        mono: true
                        text: String(App.settings.backups_to_keep === undefined
                                     ? 10 : App.settings.backups_to_keep)
                        onCommit: function (v) {
                            var n = parseInt(v, 10)
                            if (!isNaN(n))
                                page.updateSettings({ backups_to_keep: Math.max(0, n) })
                        }
                    }
                }
            }

            // ---------------------------------------------------------------
            Card {
                Layout.fillWidth: true
                title: "What the block writes"

                FieldRow {
                    width: parent.width
                    label: "Shader paths"
                    hint: "Write the folder once as a local and build paths from it, rather than " +
                          "repeating it on every line."

                    Toggle {
                        checked: App.state.useShaderDirVariable !== false
                        onToggled: App.run("config.update", { useShaderDirVariable: checked })
                    }
                }

                FieldRow {
                    width: parent.width
                    label: "Load the plugin"
                    hint: "Loading at session start rather than at config-parse time means a bad " +
                          "build costs you shaders for one session instead of your desktop."

                    Dropdown {
                        width: parent.width
                        model: [
                            { id: "none", label: "Leave it to me" },
                            { id: "hyprpm", label: "hyprpm reload -n" },
                            { id: "manual", label: "hyprctl plugin load …" }
                        ]
                        textRole: "label"
                        currentIndex: {
                            var m = App.state.loadMode || "none"
                            return m === "hyprpm" ? 1 : (m === "manual" ? 2 : 0)
                        }
                        onActivated: function (index) {
                            App.run("config.update",
                                    { loadMode: ["none", "hyprpm", "manual"][index] })
                        }
                    }
                }

                FieldRow {
                    width: parent.width
                    visible: App.state.loadMode === "manual"
                    label: "Plugin .so"

                    LineEdit {
                        width: parent.width
                        mono: true
                        placeholderText: "~/.local/share/hyprland/plugins/HyprWindowShade.so"
                        text: App.state.pluginSoPath || ""
                        onCommit: function (v) {
                            App.run("config.update", { pluginSoPath: v.trim() })
                        }
                    }
                }

                FieldRow {
                    width: parent.width
                    label: "Startup delay"
                    hint: "Zero applies layer and class shaders directly in the start handler. " +
                          "Raise it only if the plugin finishes loading after that runs — the " +
                          "calls are then re-issued through a delayed hyprctl instead."

                    LabeledSlider {
                        width: parent.width
                        from: 0
                        to: 10
                        stepSize: 0.5
                        label: App.state.startupDelaySecs > 0
                               ? "delayed by " + App.state.startupDelaySecs + "s"
                               : "no delay"
                        value: App.state.startupDelaySecs || 0
                        onSettled: function (v) {
                            App.run("config.update", { startupDelaySecs: v })
                        }
                    }
                }
            }

            // ---------------------------------------------------------------
            Card {
                Layout.fillWidth: true
                title: "Behavior"

                FieldRow {
                    width: parent.width
                    label: "Reload Hyprland after saving"
                    hint: "Runs hyprctl reload so new rules take effect without logging out."

                    Toggle {
                        checked: App.settings.reload_after_save === true
                        onToggled: page.updateSettings({ reload_after_save: checked })
                    }
                }

                FieldRow {
                    width: parent.width
                    label: "Reload shaders after editing one"
                    hint: "The plugin already reloads a shader when its mtime changes, so this is " +
                          "only worth turning on if that does not take on your setup."

                    Toggle {
                        checked: App.settings.reload_shaders_after_edit === true
                        onToggled: page.updateSettings({ reload_shaders_after_edit: checked })
                    }
                }

                FieldRow {
                    width: parent.width
                    label: "Keep it in the tray"
                    hint: "An icon in your bar's tray. Closing the window leaves hyprcustard running "
                          + "there; click the icon to open it, or right-click it to quit."

                    Toggle {
                        checked: App.settings.tray === true
                        onToggled: page.updateSettings({ tray: checked })
                    }
                }

                FieldRow {
                    width: parent.width
                    label: "Start at login"
                    hint: App.state.autostart === true
                          ? "A block at the end of " + App.state.configPathDisplay + " starts hyprcustard "
                            + "in the tray when Hyprland starts. Turning this off takes the block out."
                          : "Adds a block to the end of " + App.state.configPathDisplay + " that starts "
                            + "hyprcustard in the tray when Hyprland starts, without opening its window. "
                            + "A backup is made first."

                    Toggle {
                        checked: App.state.autostart === true
                        onToggled: App.run("app.autostart", { on: checked })
                    }
                }
            }

            // ---------------------------------------------------------------
            Card {
                Layout.fillWidth: true
                title: "This app's colors"
                subtitle: "How hyprcustard itself looks — for your Hyprland look, see the "
                          + "Look and Themes pages. Gruvbox dark and light are built in. Anything else is a TOML file in " +
                          App.state.themeDir + ", which can override as few colors as you like."

                FieldRow {
                    width: parent.width
                    label: "App colors"
                    hint: "System follows your Qt colors, so the app matches the rest of your " +
                          "desktop and changes with it. Color scheme wears the scheme last applied " +
                          "on the Colors page. The Gruvboxes look the same everywhere."

                    Dropdown {
                        width: parent.width
                        model: App.themes
                        textRole: "name"
                        currentIndex: {
                            for (var i = 0; i < App.themes.length; ++i)
                                if (App.themes[i].id === (App.state.theme && App.state.theme.id))
                                    return i
                            return 0
                        }
                        onActivated: function (index) {
                            App.run("config.update", { theme: App.themes[index].id })
                        }
                    }
                }

                RowLayout {
                    width: parent.width
                    spacing: Theme.gapSmall

                    PillButton {
                        text: "Write an example theme"
                        tooltip: "Creates example.toml in the theme folder, with every color " +
                                 "documented"
                        onClicked: App.run("theme.writeExample", {})
                    }
                    PillButton {
                        text: "Reload themes"
                        onClicked: App.run("theme.reload", {})
                    }
                }

                Flow {
                    width: parent.width
                    spacing: Theme.gapSmall

                    Repeater {
                        model: [
                            { name: "bg", c: Theme.bg },
                            { name: "surface", c: Theme.surface },
                            { name: "border", c: Theme.border },
                            { name: "fg", c: Theme.fg },
                            { name: "muted", c: Theme.muted },
                            { name: "accent", c: Theme.accent },
                            { name: "accentAlt", c: Theme.accentAlt },
                            { name: "ok", c: Theme.ok },
                            { name: "warn", c: Theme.warn },
                            { name: "error", c: Theme.error }
                        ]

                        Column {
                            required property var modelData
                            spacing: 3

                            Rectangle {
                                width: 46
                                height: 28
                                radius: Theme.radiusSmall
                                color: parent.modelData.c
                                border.width: 1
                                border.color: Theme.border
                            }
                            Text {
                                text: parent.modelData.name
                                color: Theme.muted
                                font.pixelSize: Theme.fontSizeSmall
                            }
                        }
                    }
                }
            }

            // ---------------------------------------------------------------
            Card {
                Layout.fillWidth: true
                visible: App.problems.length > 0 || App.missingShaders.length > 0
                title: "Problems"

                Repeater {
                    model: App.problems

                    Text {
                        required property string modelData
                        width: parent.width
                        text: "•  " + modelData
                        color: Theme.error
                        font.pixelSize: Theme.fontSize
                        wrapMode: Text.WordWrap
                    }
                }

                Repeater {
                    model: App.missingShaders

                    Text {
                        required property var modelData
                        width: parent.width
                        text: "•  " + modelData.path + " is referenced but not on disk"
                        color: Theme.warn
                        font.pixelSize: Theme.fontSize
                        wrapMode: Text.WordWrap
                    }
                }
            }

            Card {
                Layout.fillWidth: true
                title: "About"

                Row {
                    width: parent.width
                    spacing: Theme.gap

                    Image {
                        width: 64
                        height: 64
                        source: "qrc:/qt/qml/dev/hyprcustard/gui/icons/hyprcustard.png"
                        sourceSize.width: 128
                        sourceSize.height: 128
                        smooth: true
                    }

                    Text {
                        width: parent.width - 64 - Theme.gap
                        anchors.verticalCenter: parent.verticalCenter
                        wrapMode: Text.WordWrap
                        color: Theme.muted
                        font.pixelSize: Theme.fontSizeSmall
                        text: "hyprcustard " + App.appVersion + " — a theming GUI for Hyprland, "
                              + "including the HyprWindowShade shader plugin. It only ever edits the "
                              + "block between its own markers, and backs the file up first."
                    }
                }
            }

            Item {
                Layout.preferredHeight: Theme.pad
            }
        }
    }
}
