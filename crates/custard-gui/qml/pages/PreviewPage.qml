import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import dev.hyprcustard.gui

// Exactly what will be written, and a way to pull in rules that are already
// in the config by hand.
Item {
    id: page

    property string preview: ""
    property var importResult: null

    function refreshPreview() {
        page.preview = App.query("preview", {})
    }

    Connections {
        target: App
        function onStateChanged() {
            page.refreshPreview()
        }
    }

    Component.onCompleted: page.refreshPreview()

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
                title: "What gets written"
                subtitle: "This replaces everything between the two marker lines in " +
                          App.configPathDisplay + ". The rest of the file is left exactly as it is, " +
                          "and a timestamped backup is taken before every write."

                Rectangle {
                    width: parent.width
                    height: Math.min(560, previewText.implicitHeight + Theme.gap * 2)
                    color: Theme.bgAlt
                    radius: Theme.radiusSmall
                    border.width: 1
                    border.color: Theme.border

                    ScrollView {
                        anchors.fill: parent
                        anchors.margins: Theme.gap
                        clip: true

                        Text {
                            id: previewText
                            text: page.preview
                            color: Theme.fgDim
                            font.family: Theme.monoFamily
                            font.pixelSize: Theme.fontSizeSmall
                            textFormat: Text.PlainText
                        }
                    }
                }

                RowLayout {
                    width: parent.width
                    spacing: Theme.gapSmall

                    PillButton {
                        text: "Save to config"
                        primary: true
                        onClicked: App.run("config.save", {})
                    }
                    PillButton {
                        text: "Discard changes"
                        enabled: App.dirty
                        tooltip: "Re-read the file and throw away anything unsaved"
                        onClicked: App.confirm(
                            "Discard unsaved changes?",
                            "Your rules, layers and keybinds go back to what is in "
                            + App.configPathDisplay + " now. There is no backup of "
                            + "unsaved work, because it was never written.",
                            "Discard", true,
                            function () { App.run("config.reload", {}) })
                    }
                    Item {
                        Layout.fillWidth: true
                    }
                    PillButton {
                        text: "Remove managed block"
                        danger: true
                        tooltip: "Delete this app's section from the config, leaving the rest alone"
                        onClicked: App.confirm(
                            "Remove the managed block?",
                            "This rewrites " + App.configPathDisplay + " now, deleting everything "
                            + "between the two marker lines. The rest of the file is untouched and "
                            + "a timestamped backup is taken first, but the app will no longer "
                            + "know about your rules.",
                            "Remove it", true,
                            function () { App.run("config.removeBlock", {}) })
                    }
                }
            }

            // ---------------------------------------------------------------
            // Import
            // ---------------------------------------------------------------
            Card {
                Layout.fillWidth: true
                title: "Import rules you wrote by hand"
                subtitle: "Reads the parts of your config outside the managed block. It understands " +
                          "string literals and concatenations with locals defined in the same file, " +
                          "and skips anything it cannot read rather than guessing."

                RowLayout {
                    width: parent.width
                    spacing: Theme.gapSmall

                    PillButton {
                        text: "Look for rules"
                        onClicked: page.importResult = App.queryJson("importPreview", {})
                    }

                    PillButton {
                        text: "Import them"
                        primary: true
                        visible: page.importResult !== null
                                 && !page.importResult.error
                                 && page.importResult.empty === false
                        onClicked: {
                            App.run("import.apply", {})
                            page.importResult = null
                        }
                    }
                }

                Text {
                    width: parent.width
                    visible: page.importResult !== null
                    wrapMode: Text.WordWrap
                    color: page.importResult && page.importResult.error ? Theme.error : Theme.fgDim
                    font.pixelSize: Theme.fontSize
                    text: {
                        if (!page.importResult)
                            return ""
                        if (page.importResult.error)
                            return page.importResult.error
                        if (page.importResult.empty)
                            return "Nothing found outside the managed block."
                        return "Found " + page.importResult.summary
                                + ". Importing adds them here; nothing is written until you save, " +
                                "and your original lines are left in place for you to remove."
                    }
                }

                Repeater {
                    model: page.importResult && page.importResult.rules
                           ? page.importResult.rules : []

                    Text {
                        required property var modelData
                        width: parent.width
                        wrapMode: Text.WordWrap
                        color: Theme.fgDim
                        font.family: Theme.monoFamily
                        font.pixelSize: Theme.fontSizeSmall
                        text: "• " + modelData.match + "  →  " + modelData.tags.join(", ")
                    }
                }

                Repeater {
                    model: page.importResult && page.importResult.layers
                           ? page.importResult.layers : []

                    Text {
                        required property var modelData
                        width: parent.width
                        wrapMode: Text.WordWrap
                        color: Theme.fgDim
                        font.family: Theme.monoFamily
                        font.pixelSize: Theme.fontSizeSmall
                        text: "• layer " + modelData.namespace
                              + (modelData.shader ? "  shader " + modelData.shader : "")
                              + (modelData.openAnim ? "  open " + modelData.openAnim : "")
                              + (modelData.closeAnim ? "  close " + modelData.closeAnim : "")
                    }
                }

                Repeater {
                    model: page.importResult && page.importResult.binds
                           ? page.importResult.binds : []

                    Text {
                        required property var modelData
                        width: parent.width
                        wrapMode: Text.WordWrap
                        color: Theme.fgDim
                        font.family: Theme.monoFamily
                        font.pixelSize: Theme.fontSizeSmall
                        text: "• " + modelData.key + "  →  " + modelData.summary
                    }
                }

                Repeater {
                    model: page.importResult && page.importResult.skipped
                           ? page.importResult.skipped : []

                    Text {
                        required property string modelData
                        width: parent.width
                        wrapMode: Text.WordWrap
                        color: Theme.warn
                        font.pixelSize: Theme.fontSizeSmall
                        text: "skipped: " + modelData
                    }
                }
            }

            Item {
                Layout.preferredHeight: Theme.pad
            }
        }
    }
}
