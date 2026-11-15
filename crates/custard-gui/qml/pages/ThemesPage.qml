import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import QtQuick.Dialogs
import dev.hyprcustard.gui

// Themes: the whole look — Look options, animations and curves, and if asked
// for the shaders, the colors and the wallpaper — saved under a name and put
// back with one click.
//
// Applying a theme makes the look block set everything the theme holds, so
// the desktop looks the way the theme was saved whatever the user's own
// config says. Like any other change to the look it is live at once and
// written on Save. Colors and wallpaper are not in the Hyprland config, so
// they go on, and stay, at once — as they do on their own pages.
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

    // What a save takes from the desktop besides the look, as a phrase.
    function partsSaved() {
        var parts = ["the look, animations and look rules"]
        if (withShaders.checked)
            parts.push("shaders")
        if (withColors.checked)
            parts.push("colors")
        if (withWallpaper.checked)
            parts.push("wallpaper")
        if (parts.length === 1)
            return parts[0]
        return parts.slice(0, -1).join(", ") + " and " + parts[parts.length - 1]
    }

    function saveOptions() {
        return {
            includeShaders: withShaders.checked,
            includeColors: withColors.checked,
            includeWallpaper: withWallpaper.checked
        }
    }

    // Overwrite one of the user's themes with what is on the desktop now,
    // keeping its name, description and file.
    function update(theme) {
        var lost = []
        if (theme.hasShaders && !withShaders.checked)
            lost.push("shaders")
        if (theme.colors && !withColors.checked)
            lost.push("colors")
        if (theme.wallpaper && !withWallpaper.checked)
            lost.push("wallpaper")
        var body = "Replace it with what is on the desktop now: " + page.partsSaved()
                   + ", unsaved changes included. Its name and description stay."
        if (lost.length > 0)
            body += " It will no longer carry its " + lost.join(" or ")
                    + " — turn on Include above to keep them."
        App.confirm("Update " + theme.name + "?", body, "Update", lost.length > 0,
                    function () {
                        var opts = page.saveOptions()
                        opts.name = theme.name
                        opts.file = theme.file
                        App.run("preset.save", opts)
                    })
    }

    // What Apply does with a theme, for its tooltip.
    function applyTip(theme) {
        var parts = []
        if (theme.hasShaders)
            parts.push("its shaders")
        if (theme.colors)
            parts.push("its colors")
        if (theme.wallpaper)
            parts.push("its wallpaper")
        if (parts.length === 0)
            return ""
        return "The look, and " + parts.join(", ") + "."
            + (theme.colors || theme.wallpaper
               ? " Colors and wallpaper change at once; the look waits for Save." : "")
    }

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

    PageScroll {
        id: scroll
        anchors.fill: parent

        ColumnLayout {
            width: Math.min(scroll.availableWidth, Theme.contentMax)
            x: Math.round((scroll.availableWidth - width) / 2)
            spacing: Theme.pad

            Card {
                Layout.fillWidth: true
                title: "Themes"
                subtitle: "A theme is the whole look: every Look option, the animations and "
                          + "their curves, your look rules — and, if you include them, the "
                          + "HyprWindowShade rules and shader files, the desktop's colors and the "
                          + "wallpaper. Themes are files in " + page.presetDir + "."

                SaveNote {
                    width: parent.width
                    label: "Two ways"
                    text: "Saving, updating and deleting a theme happen at once. Applying one "
                          + "shows its look straight away but keeps it only when you press Save "
                          + "at the top; its colors and wallpaper go on, and stay, at once."
                }

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
                        tooltip: "Save " + page.partsSaved() + " as they are now, unsaved "
                                 + "changes included, as a new theme."
                        onClicked: {
                            var name = nameEdit.text.trim()
                            var taken = false
                            for (var i = 0; i < page.presets.length; ++i)
                                if (!page.presets[i].builtin && page.presets[i].name === name)
                                    taken = true
                            var save = function (replace) {
                                var opts = page.saveOptions()
                                opts.name = name
                                opts.description = descEdit.text
                                opts.replace = replace
                                App.run("preset.save", opts)
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

                Flow {
                    width: parent.width
                    spacing: Theme.gap
                    Toggle {
                        id: withShaders
                        text: "Include shaders"
                        checked: true
                    }
                    Toggle {
                        id: withColors
                        text: "Include colors"
                        checked: true
                        tooltip: "The scheme last applied on the Colors page, and the choices "
                                 + "that made it."
                    }
                    Toggle {
                        id: withWallpaper
                        text: "Include wallpaper"
                        checked: true
                        tooltip: "The wallpaper on screen, kept as a path to the file: a theme "
                                 + "shared with someone else has no wallpaper on their machine."
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
                spacing: Theme.cardGap

                Repeater {
                    model: page.presets

                    Rectangle {
                        id: themeCard

                        required property var modelData

                        width: 300
                        height: cardColumn.implicitHeight + Theme.pad * 2
                        radius: Theme.radius
                        color: "transparent"

                        Raised {
                            anchors.fill: parent
                            z: -1
                            radius: parent.radius
                            sunk: themeCard.modelData.active
                        }

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
                            Flow {
                                width: parent.width
                                spacing: Theme.gapSmall
                                visible: themeCard.modelData.active || themeCard.modelData.builtin
                                         || themeCard.modelData.hasShaders
                                         || !!themeCard.modelData.colors
                                         || !!themeCard.modelData.wallpaper

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
                                Badge {
                                    visible: !!themeCard.modelData.colors
                                    text: "Colors"
                                    tint: Theme.accentAlt
                                }
                                Badge {
                                    visible: !!themeCard.modelData.wallpaper
                                    text: "Wallpaper"
                                    tint: themeCard.modelData.wallpaper
                                          && themeCard.modelData.wallpaper.missing
                                          ? Theme.warn : Theme.accentAlt
                                }
                            }

                            // The theme's colors: background, then accents.
                            Row {
                                visible: !!themeCard.modelData.colors
                                spacing: 3
                                Repeater {
                                    model: themeCard.modelData.colors
                                           ? themeCard.modelData.colors.swatches : []
                                    Rectangle {
                                        required property var modelData
                                        width: 16
                                        height: 16
                                        radius: 4
                                        color: modelData
                                        border.width: 1
                                        border.color: Theme.wash(Theme.fg, 0.15)
                                    }
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
                                      + (themeCard.modelData.colors
                                         ? " · colors: " + themeCard.modelData.colors.name : "")
                                color: Theme.muted
                                font.pixelSize: Theme.fontSizeSmall
                            }
                            Text {
                                width: parent.width
                                visible: !!themeCard.modelData.wallpaper
                                wrapMode: Text.WrapAnywhere
                                text: !themeCard.modelData.wallpaper ? ""
                                      : "Wallpaper: " + themeCard.modelData.wallpaper.name
                                        + (themeCard.modelData.wallpaper.missing
                                           ? " — not on this machine, so it is left out" : "")
                                color: themeCard.modelData.wallpaper
                                       && themeCard.modelData.wallpaper.missing
                                       ? Theme.warn : Theme.muted
                                font.pixelSize: Theme.fontSizeSmall
                            }

                            Flow {
                                width: parent.width
                                spacing: Theme.gapSmall

                                PillButton {
                                    text: "Apply"
                                    primary: true
                                    compact: true
                                    tooltip: page.applyTip(themeCard.modelData)
                                    onClicked: page.apply(themeCard.modelData, false)
                                }
                                PillButton {
                                    text: "Look only"
                                    compact: true
                                    visible: themeCard.modelData.hasShaders
                                             || !!themeCard.modelData.colors
                                             || !!themeCard.modelData.wallpaper
                                    tooltip: "Apply the look and animations, and leave your "
                                             + "shaders, colors and wallpaper as they are."
                                    onClicked: page.apply(themeCard.modelData, true)
                                }
                                PillButton {
                                    text: "Update"
                                    compact: true
                                    visible: !themeCard.modelData.builtin
                                    tooltip: "Overwrite this theme with what is on the desktop "
                                             + "now, keeping its name. The Include switches "
                                             + "above say what goes in."
                                    onClicked: page.update(themeCard.modelData)
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
