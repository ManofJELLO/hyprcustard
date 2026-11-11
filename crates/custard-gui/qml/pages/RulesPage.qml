import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import dev.hyprcustard.gui

// Window rules: what matches, and which shaders it wears.
Item {
    id: page

    property string selectedId: ""

    readonly property var selected: App.ruleById(page.selectedId)

    // Keep a selection alive as rules come and go.
    function ensureSelection() {
        if (App.rules.length === 0) {
            page.selectedId = ""
            return
        }
        if (App.ruleById(page.selectedId) === null)
            page.selectedId = App.rules[App.rules.length - 1].id
    }

    Connections {
        target: App
        function onRulesChanged() {
            page.ensureSelection()
        }
    }

    Component.onCompleted: page.ensureSelection()

    RowLayout {
        anchors.fill: parent
        spacing: Theme.pad

        // ------------------------------------------------------------------
        // The list
        // ------------------------------------------------------------------
        ColumnLayout {
            // Pinned rather than merely preferred: a RowLayout hands leftover
            // space to whichever item will take it, and a list of rules does not
            // get better by being 900px wide.
            Layout.preferredWidth: Theme.listWidth
            Layout.minimumWidth: Theme.listWidth
            Layout.maximumWidth: Theme.listWidth
            Layout.fillHeight: true
            spacing: Theme.gap

            RowLayout {
                Layout.fillWidth: true
                spacing: Theme.gapSmall

                PillButton {
                    text: "Add rule"
                    primary: true
                    Layout.fillWidth: true
                    onClicked: App.run("rule.add", {})
                }

                PillButton {
                    text: "⧉"
                    tooltip: "Duplicate the selected rule"
                    enabled: page.selectedId !== ""
                    implicitWidth: Theme.rowHeight
                    padding: 0
                    onClicked: App.run("rule.duplicate", { id: page.selectedId })
                }

                PillButton {
                    text: "✕"
                    tooltip: "Delete the selected rule"
                    danger: true
                    enabled: page.selectedId !== ""
                    implicitWidth: Theme.rowHeight
                    padding: 0
                    onClicked: App.run("rule.remove", { id: page.selectedId })
                }
            }

            Rectangle {
                Layout.fillWidth: true
                Layout.fillHeight: true
                color: Theme.surface
                radius: Theme.radius
                border.width: 1
                border.color: Theme.border

                EmptyState {
                    anchors.centerIn: parent
                    width: parent.width - Theme.pad * 2
                    visible: App.rules.length === 0
                    title: "No rules yet"
                    body: "Add a rule to put a shader on a window class."
                }

                ListView {
                    id: list
                    anchors.fill: parent
                    anchors.margins: 1
                    clip: true
                    model: App.rules
                    spacing: 0
                    currentIndex: -1

                    delegate: Rectangle {
                        id: row

                        required property var modelData
                        required property int index

                        width: ListView.view.width
                        height: 68
                        color: row.modelData.id === page.selectedId
                               ? Theme.selection
                               : (hover.hovered ? Theme.surfaceHi : "transparent")

                        HoverHandler {
                            id: hover
                        }

                        MouseArea {
                            anchors.fill: parent
                            onClicked: page.selectedId = row.modelData.id
                        }

                        Rectangle {
                            width: 3
                            height: parent.height
                            color: Theme.accent
                            visible: row.modelData.id === page.selectedId
                        }

                        ColumnLayout {
                            anchors.fill: parent
                            anchors.margins: Theme.gap
                            anchors.leftMargin: Theme.gap + 4
                            spacing: 2

                            RowLayout {
                                Layout.fillWidth: true
                                spacing: Theme.gapSmall

                                Text {
                                    Layout.fillWidth: true
                                    text: row.modelData.name !== ""
                                          ? row.modelData.name
                                          : row.modelData.matchSummary
                                    color: row.modelData.enabled ? Theme.fg : Theme.muted
                                    font.pixelSize: Theme.fontSize
                                    font.weight: Font.DemiBold
                                    elide: Text.ElideRight
                                }

                                Badge {
                                    visible: !row.modelData.enabled
                                    text: "off"
                                    tint: Theme.muted
                                }
                            }

                            Text {
                                Layout.fillWidth: true
                                text: row.modelData.matchSummary
                                color: Theme.muted
                                font.pixelSize: Theme.fontSizeSmall
                                elide: Text.ElideRight
                            }

                            Row {
                                Layout.fillWidth: true
                                spacing: 4

                                Repeater {
                                    model: row.modelData.tags.slice(0, 4)

                                    Badge {
                                        required property var modelData
                                        text: modelData.label
                                        tint: Theme.accentAlt
                                    }
                                }

                                Badge {
                                    visible: row.modelData.tags.length > 4
                                    text: "+" + (row.modelData.tags.length - 4)
                                    tint: Theme.muted
                                }

                                Badge {
                                    visible: row.modelData.tags.length === 0
                                    text: "no shader set"
                                    tint: Theme.warn
                                }
                            }
                        }
                    }

                    ScrollBar.vertical: ScrollBar {}
                }
            }

            RowLayout {
                Layout.fillWidth: true
                visible: App.rules.length > 1
                spacing: Theme.gapSmall

                PillButton {
                    text: "Move up"
                    Layout.fillWidth: true
                    enabled: page.selectedId !== ""
                    onClicked: App.run("rule.move", { id: page.selectedId, delta: -1 })
                }
                PillButton {
                    text: "Move down"
                    Layout.fillWidth: true
                    enabled: page.selectedId !== ""
                    onClicked: App.run("rule.move", { id: page.selectedId, delta: 1 })
                }
            }
        }

        // ------------------------------------------------------------------
        // The editor
        // ------------------------------------------------------------------
        Item {
            Layout.fillWidth: true
            Layout.fillHeight: true

            EmptyState {
                anchors.centerIn: parent
                width: Math.min(420, parent.width - Theme.pad * 2)
                visible: page.selected === null
                title: "Nothing selected"
                body: "Pick a rule on the left, or add one."
            }

            ScrollView {
                id: editorScroll
                anchors.fill: parent
                visible: page.selected !== null
                clip: true
                contentWidth: availableWidth

                ColumnLayout {
                    width: Math.min(editorScroll.availableWidth, Theme.contentMax)
                    x: Math.round((editorScroll.availableWidth - width) / 2)
                    spacing: Theme.pad

                    // --- identity and match ---
                    Card {
                        Layout.fillWidth: true
                        title: "Matches"
                        subtitle: "Hyprland matches these as regular expressions. " +
                                  "Leave a field empty to ignore it."

                        FieldRow {
                            width: parent.width
                            label: "Enabled"
                            hint: "A disabled rule is kept here but not written to the config."

                            Toggle {
                                checked: page.selected ? page.selected.enabled : true
                                onToggled: App.run("rule.setEnabled",
                                                   { id: page.selectedId, enabled: checked })
                            }
                        }

                        FieldRow {
                            width: parent.width
                            label: "Name"
                            hint: "Shown in hyprctl's rule listing. Optional."

                            LineEdit {
                                width: parent.width
                                text: page.selected ? page.selected.name : ""
                                placeholderText: "kitty-reading-mode"
                                onCommit: function (v) {
                                    App.run("rule.setName", { id: page.selectedId, name: v })
                                }
                            }
                        }

                        FieldRow {
                            width: parent.width
                            label: "class"
                            hint: App.hyprlandRunning
                                  ? "Open windows are offered in the list."
                                  : "Hyprland is not running, so there is nothing to offer."

                            RowLayout {
                                width: parent.width
                                spacing: Theme.gapSmall

                                LineEdit {
                                    id: classField
                                    Layout.fillWidth: true
                                    mono: true
                                    text: page.selected && page.selected.match["class"]
                                          ? page.selected.match["class"] : ""
                                    placeholderText: "^(kitty)$"
                                    onCommit: function (v) {
                                        App.run("rule.setMatch", { id: page.selectedId, "class": v })
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
                                        var v = App.liveClasses[index - 1]
                                        App.run("rule.setMatch", { id: page.selectedId, "class": v })
                                        currentIndex = 0
                                    }
                                }
                            }
                        }

                        FieldRow {
                            width: parent.width
                            label: "title"

                            LineEdit {
                                width: parent.width
                                mono: true
                                text: page.selected && page.selected.match["title"]
                                      ? page.selected.match["title"] : ""
                                onCommit: function (v) {
                                    App.run("rule.setMatch", { id: page.selectedId, title: v })
                                }
                            }
                        }

                        FieldRow {
                            width: parent.width
                            label: "initialClass"

                            LineEdit {
                                width: parent.width
                                mono: true
                                text: page.selected && page.selected.match["initial_class"]
                                      ? page.selected.match["initial_class"] : ""
                                onCommit: function (v) {
                                    App.run("rule.setMatch",
                                            { id: page.selectedId, initialClass: v })
                                }
                            }
                        }

                        FieldRow {
                            width: parent.width
                            label: "initialTitle"

                            LineEdit {
                                width: parent.width
                                mono: true
                                text: page.selected && page.selected.match["initial_title"]
                                      ? page.selected.match["initial_title"] : ""
                                onCommit: function (v) {
                                    App.run("rule.setMatch",
                                            { id: page.selectedId, initialTitle: v })
                                }
                            }
                        }

                        Rectangle {
                            width: parent.width
                            height: warnText.implicitHeight + Theme.gap * 2
                            visible: page.selected !== null && page.selected.matchesEverything
                            radius: Theme.radiusSmall
                            color: Theme.wash(Theme.warn, 0.14)
                            border.width: 1
                            border.color: Theme.wash(Theme.warn, 0.5)

                            Text {
                                id: warnText
                                anchors.fill: parent
                                anchors.margins: Theme.gap
                                wrapMode: Text.WordWrap
                                color: Theme.warn
                                font.pixelSize: Theme.fontSizeSmall
                                text: "Nothing is set, so this rule is written as class \".*\" and " +
                                      "applies to every window. If that is not what you want, " +
                                      "fill in a class."
                            }
                        }
                    }

                    // --- tags ---
                    Repeater {
                        model: App.tagGroups

                        Card {
                            id: groupCard

                            required property var modelData

                            Layout.fillWidth: true
                            title: groupCard.modelData.label
                            spacing: Theme.gap

                            Repeater {
                                model: groupCard.modelData.tags

                                Item {
                                    id: tagRow

                                    required property var modelData

                                    readonly property var tag: App.tagOf(page.selected,
                                                                         tagRow.modelData.key)
                                    readonly property bool isSet: tagRow.tag !== null

                                    width: parent.width
                                    implicitHeight: tagColumn.implicitHeight

                                    Column {
                                        id: tagColumn
                                        width: parent.width
                                        spacing: 3

                                        RowLayout {
                                            width: parent.width
                                            spacing: Theme.gapSmall

                                            Text {
                                                Layout.preferredWidth: 150
                                                text: tagRow.modelData.label
                                                color: tagRow.isSet ? Theme.fg : Theme.fgDim
                                                font.pixelSize: Theme.fontSize
                                                font.weight: tagRow.isSet ? Font.DemiBold : Font.Normal
                                                elide: Text.ElideRight

                                                HoverHandler {
                                                    id: tagHover
                                                }
                                                Tip {
                                                    visible: tagHover.hovered
                                                    delay: 400
                                                    text: tagRow.modelData.help
                                                }
                                            }

                                            // Flag-valued tags are a switch.
                                            Toggle {
                                                Layout.fillWidth: true
                                                visible: tagRow.modelData.kind === "flag"
                                                checked: tagRow.isSet
                                                onToggled: {
                                                    App.run("rule.setTag", {
                                                        id: page.selectedId,
                                                        slot: tagRow.modelData.key,
                                                        flag: checked
                                                    })
                                                }
                                            }

                                            // Path-valued tags are a shader picker.
                                            ShaderPicker {
                                                Layout.fillWidth: true
                                                visible: tagRow.modelData.kind === "path"
                                                path: tagRow.tag && tagRow.tag.path
                                                      ? tagRow.tag.path : ""
                                                duration: tagRow.tag ? tagRow.tag.duration : null
                                                isDefault: tagRow.tag
                                                           ? tagRow.tag.isDefault === true : false
                                                showDuration: tagRow.modelData.takesDuration
                                                showDefault: tagRow.modelData.supportsDefault
                                                onPicked: function (path, duration, isDefault) {
                                                    if (path === "") {
                                                        App.run("rule.clearTag", {
                                                            id: page.selectedId,
                                                            slot: tagRow.modelData.key
                                                        })
                                                        return
                                                    }
                                                    App.run("rule.setTag", {
                                                        id: page.selectedId,
                                                        slot: tagRow.modelData.key,
                                                        path: path,
                                                        duration: duration,
                                                        isDefault: isDefault
                                                    })
                                                }
                                            }
                                        }

                                        // Warnings that only matter once the tag is set.
                                        Text {
                                            width: parent.width
                                            visible: tagRow.isSet
                                                     && tagRow.tag.isDefault === true
                                                     && tagRow.modelData.supportsDefault
                                                     && !tagRow.modelData.defaultDocumented
                                            leftPadding: 158
                                            wrapMode: Text.WordWrap
                                            color: Theme.warn
                                            font.pixelSize: Theme.fontSizeSmall
                                            text: "The plugin documents _default for the eight core " +
                                                  "tags; this one is not among them. Test it before " +
                                                  "relying on it."
                                        }

                                        Text {
                                            width: parent.width
                                            visible: {
                                                if (!tagRow.isSet || !tagRow.tag.path)
                                                    return false
                                                var s = App.shaderByPath(tagRow.tag.path)
                                                return s !== null && s.isAnimation
                                                       && s.duration === null
                                                       && (tagRow.tag.duration === null
                                                           || tagRow.tag.duration === undefined)
                                            }
                                            leftPadding: 158
                                            wrapMode: Text.WordWrap
                                            color: Theme.warn
                                            font.pixelSize: Theme.fontSizeSmall
                                            text: "This shader animates on progress but declares no " +
                                                  "duration, so the plugin uses 0.3s and warns once " +
                                                  "per edit. Set one here or in the shader."
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
    }
}
