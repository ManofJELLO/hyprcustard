import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import QtQuick.Dialogs
import dev.hyprcustard.gui

// Themes: the whole look — Look options, animations and curves — saved under
// a name and put back with one click.
//
// Applying a theme makes the look block set everything the theme holds, so
// the desktop looks the way the theme was saved whatever the user's own
// config says. Like any other change it is live at once and written on Save.
Item {
    id: page

    readonly property var presets: App.state.presets || []
    readonly property string presetDir: App.state.presetDir || ""

    // A file:// URL from a dialog as a plain path.
    function localPath(url) {
        var s = String(url)
        if (s.indexOf("file://") === 0)
            s = s.substring(7)
        return decodeURIComponent(s)
    }

    // The theme an export dialog was opened for.
    property var exporting: null

    // Apply a theme, asking first if it would change shaders the user
    // already has different versions of.
    function apply(theme, lookOnly) {
        var pick = { name: theme.name, file: theme.file || "", lookOnly: lookOnly === true }
        if (!theme.hasShaders || lookOnly) {
            App.run("preset.apply", pick)
            return
        }
        var conflicts = App.queryJson("presetConflicts", pick)
        if (!Array.isArray(conflicts) || conflicts.length === 0) {
            App.run("preset.apply", pick)
            return
        }
        App.choose(
            "Change your shaders?",
            theme.name + " has its own version of " + conflicts.join(", ")
            + ", and yours is different. Replace yours (it is backed up first), or keep "
            + "yours and add the theme's beside it under a new name?",
            "Replace mine", true,
            function () {
                pick.onConflict = "replace"
                App.run("preset.apply", pick)
            },
            "Keep both",
            function () {
                pick.onConflict = "keep_both"
                App.run("preset.apply", pick)
            })
    }

    FileDialog {
        id: importDialog
        title: "Import a theme"
        fileMode: FileDialog.OpenFile
        nameFilters: ["Themes (*.json)", "All files (*)"]
        onAccepted: App.run("preset.import", { path: page.localPath(importDialog.selectedFile) })
    }

    FileDialog {
        id: exportDialog
        title: "Export a theme"
        fileMode: FileDialog.SaveFile
        nameFilters: ["Themes (*.json)"]
        defaultSuffix: "json"
        onAccepted: {
            if (page.exporting)
                App.run("preset.export", {
                    name: page.exporting.name,
                    file: page.exporting.file || "",
                    to: page.localPath(exportDialog.selectedFile)
                })
        }
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

            Card {
                Layout.fillWidth: true
                title: "Themes"
                subtitle: "A theme is the whole look: every Look option, the animations and "
                          + "their curves, your look rules — and, if you include them, the HyprWindowShade rules, "
                          + "layer shaders and the shader files they use. Applying one puts it on the desktop straight away; "
                          + "Save writes it to hyprland.lua. Themes are files in "
                          + page.presetDir + "."

                RowLayout {
                    width: parent.width
                    spacing: Theme.gap

                    LineEdit {
                        id: nameEdit
                        Layout.preferredWidth: 220
                        placeholderText: "Name for the current look"
                        onCommit: function (v) {}
                    }
                    LineEdit {
                        id: descEdit
                        Layout.fillWidth: true
                        placeholderText: "A line about it (optional)"
                        onCommit: function (v) {}
                    }
                    PillButton {
                        text: "Save as theme"
                        primary: true
                        enabled: nameEdit.text.trim() !== ""
                        tooltip: "Save what is on the desktop now, unsaved changes included, "
                                 + "as a theme."
                        onClicked: {
                            var name = nameEdit.text.trim()
                            var taken = false
                            for (var i = 0; i < page.presets.length; ++i)
                                if (!page.presets[i].builtin && page.presets[i].name === name)
                                    taken = true
                            var save = function (replace) {
                                App.run("preset.save", {
                                    name: name, description: descEdit.text, replace: replace,
                                    includeShaders: withShaders.checked
                                })
                                nameEdit.text = ""
                                descEdit.text = ""
                            }
                            if (taken)
                                App.confirm("Replace " + name + "?",
                                            "There is already a theme with that name.",
                                            "Replace", true, function () { save(true) })
                            else
                                save(false)
                        }
                    }
                }

                Row {
                    spacing: Theme.gap
                    Toggle {
                        id: withShaders
                        text: "Include shaders"
                        checked: true
                        anchors.verticalCenter: parent.verticalCenter
                    }
                    PillButton {
                        text: "Import…"
                        tooltip: "Copy a theme file into your themes."
                        onClicked: importDialog.open()
                    }
                    PillButton {
                        text: "Reload"
                        tooltip: "Read the themes folder again, after editing a file by hand."
                        onClicked: App.run("preset.rescan", {})
                    }
                }
            }

            Flow {
                Layout.fillWidth: true
                spacing: Theme.pad

                Repeater {
                    model: page.presets

                    Rectangle {
                        id: themeCard

                        required property var modelData

                        width: 300
                        height: cardColumn.implicitHeight + Theme.pad * 2
                        radius: Theme.radius
                        color: Theme.surface
                        border.width: themeCard.modelData.active ? 2 : 1
                        border.color: themeCard.modelData.active ? Theme.accent : Theme.border

                        Column {
                            id: cardColumn
                            anchors.fill: parent
                            anchors.margins: Theme.pad
                            spacing: Theme.gapSmall

                            ThemePreview {
                                width: parent.width
                                height: 150
                                options: themeCard.modelData.options
                            }

                            FitText {
                                width: parent.width
                                text: themeCard.modelData.name
                                color: Theme.fg
                                font.pixelSize: Theme.fontSizeLarge
                                font.weight: Font.DemiBold
                                elide: Text.ElideRight
                            }
                            Row {
                                spacing: Theme.gapSmall
                                visible: themeCard.modelData.active || themeCard.modelData.builtin
                                         || themeCard.modelData.hasShaders

                                Badge {
                                    visible: themeCard.modelData.active
                                    text: "In use"
                                }
                                Badge {
                                    visible: themeCard.modelData.builtin
                                    text: "Built in"
                                    tint: Theme.muted
                                }
                                Badge {
                                    visible: themeCard.modelData.hasShaders
                                    text: "Shaders"
                                    tint: Theme.accentAlt
                                }
                            }

                            Text {
                                width: parent.width
                                visible: text !== ""
                                text: themeCard.modelData.description
                                wrapMode: Text.WordWrap
                                color: Theme.fgDim
                                font.pixelSize: Theme.fontSizeSmall
                            }
                            Text {
                                width: parent.width
                                wrapMode: Text.WordWrap
                                text: App.plural(themeCard.modelData.animationCount, "animation")
                                      + " · " + App.plural(themeCard.modelData.curveCount, "curve")
                                      + (themeCard.modelData.lookRuleCount > 0
                                         ? " · " + App.plural(themeCard.modelData.lookRuleCount, "look rule")
                                         : "")
                                      + (themeCard.modelData.hasShaders
                                         ? " · " + App.plural(themeCard.modelData.ruleCount, "rule")
                                           + " · " + App.plural(themeCard.modelData.layerCount, "layer")
                                           + " · " + App.plural(themeCard.modelData.shaderFiles, "shader")
                                         : "")
                                color: Theme.muted
                                font.pixelSize: Theme.fontSizeSmall
                            }

                            Row {
                                spacing: Theme.gapSmall

                                PillButton {
                                    text: "Apply"
                                    primary: true
                                    compact: true
                                    tooltip: themeCard.modelData.hasShaders
                                             ? "The look and the shaders." : ""
                                    onClicked: page.apply(themeCard.modelData, false)
                                }
                                PillButton {
                                    text: "Look only"
                                    compact: true
                                    visible: themeCard.modelData.hasShaders
                                    tooltip: "Apply the look and animations, and leave your "
                                             + "shaders as they are."
                                    onClicked: page.apply(themeCard.modelData, true)
                                }
                                PillButton {
                                    text: "Export…"
                                    compact: true
                                    onClicked: {
                                        page.exporting = themeCard.modelData
                                        exportDialog.open()
                                    }
                                }
                                PillButton {
                                    text: "Delete"
                                    compact: true
                                    danger: true
                                    visible: !themeCard.modelData.builtin
                                    onClicked: App.confirm(
                                        "Delete " + themeCard.modelData.name + "?",
                                        "This removes its file. The desktop keeps whatever it "
                                        + "looks like now.",
                                        "Delete", true,
                                        function () {
                                            App.run("preset.delete", {
                                                name: themeCard.modelData.name,
                                                file: themeCard.modelData.file
                                            })
                                        })
                                }
                            }
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
