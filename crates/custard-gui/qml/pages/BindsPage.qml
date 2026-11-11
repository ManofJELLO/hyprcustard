import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import dev.hyprcustard.gui

// Keybinds, and the calls made once at session start.
Item {
    id: page

    function updateBind(bind, changes) {
        var copy = { id: bind.id, enabled: bind.enabled, key: bind.key, action: bind.action }
        for (var k in changes)
            copy[k] = changes[k]
        App.run("bind.update", { bind: copy })
    }

    function updateStartup(entry, changes) {
        var copy = { id: entry.id, enabled: entry.enabled, action: entry.action }
        for (var k in changes)
            copy[k] = changes[k]
        App.run("startup.update", { startup: copy })
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
            // Keybinds
            // ---------------------------------------------------------------
            Card {
                Layout.fillWidth: true
                title: "Keybinds"
                subtitle: "Each bind looks the plugin up when the key is pressed rather than when " +
                          "the config is read, so it still exists — and says so — if the plugin " +
                          "failed to load."

                PillButton {
                    text: "Add keybind"
                    primary: true
                    onClicked: App.run("bind.add", {})
                }
            }

            EmptyState {
                Layout.fillWidth: true
                visible: App.binds.length === 0
                title: "No keybinds"
                body: "Add one to toggle a shader without opening this app."
            }

            Repeater {
                model: App.binds

                Card {
                    id: bindCard

                    required property var modelData

                    Layout.fillWidth: true

                    RowLayout {
                        width: parent.width
                        spacing: Theme.gap

                        LineEdit {
                            Layout.preferredWidth: 200
                            mono: true
                            placeholderText: "SUPER + W"
                            text: bindCard.modelData.key
                            onCommit: function (v) {
                                page.updateBind(bindCard.modelData, { key: v.trim() })
                            }
                        }

                        Toggle {
                            text: "enabled"
                            checked: bindCard.modelData.enabled
                            onToggled: page.updateBind(bindCard.modelData, { enabled: checked })
                        }

                        Item {
                            Layout.fillWidth: true
                        }

                        PillButton {
                            text: "Try it"
                            enabled: App.hyprlandRunning && App.pluginLoaded
                            tooltip: App.hyprlandRunning
                                     ? "Run this action now, without saving"
                                     : "Hyprland is not running"
                            onClicked: App.run("live.apply", { action: bindCard.modelData.action })
                        }

                        PillButton {
                            text: "Remove"
                            danger: true
                            onClicked: App.run("bind.remove", { id: bindCard.modelData.id })
                        }
                    }

                    ActionEditor {
                        width: parent.width
                        action: bindCard.modelData.action
                        onEdited: function (action) {
                            page.updateBind(bindCard.modelData, { action: action })
                        }
                    }
                }
            }

            // ---------------------------------------------------------------
            // Startup actions
            // ---------------------------------------------------------------
            Card {
                Layout.fillWidth: true
                Layout.topMargin: Theme.pad
                title: "At session start"
                subtitle: "Class shaders and other one-off calls, run from hyprland.start. " +
                          "Layer shaders have their own page and are written here automatically."

                PillButton {
                    text: "Add startup action"
                    primary: true
                    onClicked: App.run("startup.add", {})
                }
            }

            Repeater {
                model: App.startup

                Card {
                    id: startCard

                    required property var modelData

                    Layout.fillWidth: true

                    RowLayout {
                        width: parent.width
                        spacing: Theme.gap

                        Toggle {
                            text: "enabled"
                            checked: startCard.modelData.enabled
                            onToggled: page.updateStartup(startCard.modelData, { enabled: checked })
                        }

                        Item {
                            Layout.fillWidth: true
                        }

                        PillButton {
                            text: "Try it"
                            enabled: App.hyprlandRunning && App.pluginLoaded
                            onClicked: App.run("live.apply", { action: startCard.modelData.action })
                        }

                        PillButton {
                            text: "Remove"
                            danger: true
                            onClicked: App.run("startup.remove", { id: startCard.modelData.id })
                        }
                    }

                    ActionEditor {
                        width: parent.width
                        action: startCard.modelData.action
                        onEdited: function (action) {
                            page.updateStartup(startCard.modelData, { action: action })
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
