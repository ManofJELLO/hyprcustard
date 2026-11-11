import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import dev.hyprcustard.gui

// The shader folder: what is in it, and the numbers inside each file.
//
// The plugin has no channel for custom uniforms, so a tunable value is a `const`
// in the source, and changing one means rewriting a line in the user's own file.
// So edits are staged rather than written: every value here can be moved,
// compared against what the file says, put back one at a time, and written as a
// group when the shader is saved. The plugin reloads a shader when its mtime
// changes, so saving is the moment it reaches the screen.
Item {
    id: page

    property string selectedPath: ""
    property string filter: ""
    property bool showSource: false

    readonly property var selected: App.shaderByPath(page.selectedPath)
    readonly property bool selectedDirty: page.selected ? page.selected.dirty === true : false

    // Where the preview goes. Beside the parameters when there is room for both
    // to be useful, underneath when there is not — a 300px column of sliders
    // next to a 300px pane serves neither.
    property bool sideBySide: true
    readonly property int selectedStaged: page.selected ? (page.selected.staged || 0) : 0

    function revert(item) {
        App.run("shader.revert", { path: page.selectedPath, item: item })
    }

    readonly property var visibleShaders: {
        var needle = page.filter.toLowerCase()
        if (needle === "")
            return App.shaders
        var out = []
        for (var i = 0; i < App.shaders.length; ++i) {
            var s = App.shaders[i]
            if (s.name.toLowerCase().indexOf(needle) >= 0
                    || s.path.toLowerCase().indexOf(needle) >= 0
                    || (s.description || "").toLowerCase().indexOf(needle) >= 0)
                out.push(s)
        }
        return out
    }

    function ensureSelection() {
        if (App.shaders.length === 0) {
            page.selectedPath = ""
            return
        }
        if (App.shaderByPath(page.selectedPath) === null)
            page.selectedPath = App.shaders[0].path
    }

    Connections {
        target: App
        function onShadersChanged() {
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

            LineEdit {
                Layout.fillWidth: true
                placeholderText: "Search shaders…"
                onTextChanged: page.filter = text
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
                    visible: page.visibleShaders.length === 0
                    title: App.shaders.length === 0 ? "No shaders found" : "Nothing matches"
                    body: App.shaders.length === 0
                          ? "Put .glsl files in " + App.shaderDirDisplay
                            + ", or point the shader folder somewhere else in Settings."
                          : "Try a different search."
                }

                ListView {
                    anchors.fill: parent
                    anchors.margins: 1
                    clip: true
                    model: page.visibleShaders

                    delegate: Rectangle {
                        id: shaderRow

                        required property var modelData

                        width: ListView.view.width
                        height: 58
                        color: shaderRow.modelData.path === page.selectedPath
                               ? Theme.selection
                               : (shaderHover.hovered ? Theme.surfaceHi : "transparent")

                        HoverHandler {
                            id: shaderHover
                        }
                        MouseArea {
                            anchors.fill: parent
                            onClicked: page.selectedPath = shaderRow.modelData.path
                        }

                        Rectangle {
                            width: 3
                            height: parent.height
                            color: Theme.accent
                            visible: shaderRow.modelData.path === page.selectedPath
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
                                    text: shaderRow.modelData.name
                                    color: Theme.fg
                                    font.pixelSize: Theme.fontSize
                                    font.weight: Font.DemiBold
                                    elide: Text.ElideRight
                                }

                                Badge {
                                    visible: shaderRow.modelData.dirty === true
                                    text: "edited"
                                    tint: Theme.warn
                                }
                                Badge {
                                    visible: shaderRow.modelData.inUse
                                    text: "in use"
                                    tint: Theme.ok
                                }
                                Badge {
                                    visible: shaderRow.modelData.notes.length > 0
                                    text: "!"
                                    tint: Theme.warn
                                }
                            }

                            RowLayout {
                                Layout.fillWidth: true
                                spacing: Theme.gapSmall

                                Text {
                                    Layout.fillWidth: true
                                    text: App.fileName(shaderRow.modelData.path)
                                    color: Theme.muted
                                    font.pixelSize: Theme.fontSizeSmall
                                    font.family: Theme.monoFamily
                                    elide: Text.ElideMiddle
                                }

                                Text {
                                    visible: shaderRow.modelData.params.length > 0
                                    text: shaderRow.modelData.params.length + " ●"
                                    color: Theme.accent
                                    font.pixelSize: Theme.fontSizeSmall
                                }
                            }
                        }
                    }

                    ScrollBar.vertical: ScrollBar {}
                }
            }

            RowLayout {
                Layout.fillWidth: true
                spacing: Theme.gapSmall

                PillButton {
                    text: "Rescan"
                    Layout.fillWidth: true
                    onClicked: App.run("shader.rescan", {})
                }
                PillButton {
                    text: "Reload in plugin"
                    Layout.fillWidth: true
                    enabled: App.hyprlandRunning
                    tooltip: App.hyprlandRunning
                             ? "Drop the plugin's compiled shader cache"
                             : "Hyprland is not running"
                    onClicked: App.run("shader.reload", {})
                }
            }
        }

        // ------------------------------------------------------------------
        // The detail
        // ------------------------------------------------------------------
        Item {
            Layout.fillWidth: true
            Layout.fillHeight: true

            EmptyState {
                anchors.centerIn: parent
                width: Math.min(420, parent.width - Theme.pad * 2)
                visible: page.selected === null
                title: "No shader selected"
                body: "Pick one on the left to see what it does and what you can tune."
            }

            ColumnLayout {
                id: detailColumn
                anchors.fill: parent
                visible: page.selected !== null
                spacing: Theme.gap

                onWidthChanged: page.sideBySide = width > 760

                // One grid rather than two layouts with the pane in each: with
                // two columns the pane sits beside the parameters, with one it
                // falls underneath them, and nothing is built twice.
                GridLayout {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    columns: page.sideBySide ? 2 : 1
                    columnSpacing: Theme.pad
                    rowSpacing: Theme.gap

                    ScrollView {
                        id: detailScroll
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        clip: true
                        contentWidth: availableWidth

                        ColumnLayout {
                            width: Math.min(detailScroll.availableWidth, Theme.contentMax)
                            x: Math.round((detailScroll.availableWidth - width) / 2)
                            spacing: Theme.pad

                            // --- header ---
                            Card {
                                Layout.fillWidth: true
                                title: page.selected ? page.selected.name : ""
                                subtitle: page.selected ? page.selected.display : ""

                                Text {
                                    width: parent.width
                                    visible: page.selected && page.selected.description
                                    text: page.selected && page.selected.description
                                          ? page.selected.description : ""
                                    color: Theme.fgDim
                                    font.pixelSize: Theme.fontSize
                                    wrapMode: Text.WordWrap
                                }

                                Flow {
                                    width: parent.width
                                    spacing: Theme.gapSmall

                                    Badge {
                                        visible: page.selected && page.selected.isAnimation
                                        text: "one-shot animation"
                                        tint: Theme.accentAlt
                                    }
                                    Badge {
                                        visible: page.selected && page.selected.isMotionDriven
                                        text: "motion driven"
                                        tint: Theme.accent
                                    }
                                    Badge {
                                        visible: page.selected && page.selected.inUse
                                        text: "used by a rule"
                                        tint: Theme.ok
                                    }
                                    Badge {
                                        visible: page.selected
                                                 && page.selected.uniforms.indexOf("time") >= 0
                                        text: "redraws continuously"
                                        tint: Theme.warn
                                    }
                                }
                            }

                            // --- notes ---
                            Card {
                                Layout.fillWidth: true
                                visible: page.selected && page.selected.notes.length > 0
                                title: "Worth knowing"

                                Repeater {
                                    model: page.selected ? page.selected.notes : []

                                    Text {
                                        required property string modelData
                                        width: parent.width
                                        text: "•  " + modelData
                                        color: Theme.warn
                                        font.pixelSize: Theme.fontSize
                                        wrapMode: Text.WordWrap
                                    }
                                }
                            }

                            // --- animation timing ---
                            Card {
                                Layout.fillWidth: true
                                visible: page.selected !== null
                                title: "Timing"
                                subtitle: page.selected && page.selected.isAnimation
                                          ? "This shader drives itself from progress, so how long it runs " +
                                            "is part of the effect. A rule can still override it."
                                          : "This shader does not use progress, so timing only applies if " +
                                            "you use it as an open or close animation."

                                FieldRow {
                                    width: parent.width
                                    label: "Declares a duration"
                                    hint: "// @duration in the file"

                                    Row {
                                        spacing: Theme.gap

                                        Toggle {
                                            anchors.verticalCenter: parent.verticalCenter
                                            checked: page.selected && page.selected.duration !== null
                                                     && page.selected.duration !== undefined
                                            onToggled: {
                                                App.run("shader.setDuration", {
                                                    path: page.selectedPath,
                                                    seconds: checked ? 0.4 : null
                                                })
                                            }
                                        }

                                        PillButton {
                                            anchors.verticalCenter: parent.verticalCenter
                                            visible: page.selected
                                                     && page.selected.durationStaged === true
                                            compact: true
                                            text: "Revert"
                                            tooltip: page.selected && page.selected.savedDuration
                                                     ? "Back to the file's "
                                                       + App.seconds(page.selected.savedDuration)
                                                     : "Back to no duration at all, as the file has it"
                                            onClicked: page.revert("duration")
                                        }
                                    }
                                }

                                LabeledSlider {
                                    width: parent.width
                                    visible: page.selected && page.selected.duration !== null
                                             && page.selected.duration !== undefined
                                    label: "Duration (seconds) — the plugin caps this at 5"
                                    from: 0.05
                                    to: 5.0
                                    stepSize: 0.05
                                    value: page.selected && page.selected.duration ? page.selected.duration : 0.4
                                    // Where the file has it, while it differs.
                                    markValue: page.selected && page.selected.durationStaged === true
                                               ? page.selected.savedDuration : null
                                    onSettled: function (v) {
                                        App.run("shader.setDuration", { path: page.selectedPath, seconds: v })
                                    }
                                }

                                FieldRow {
                                    width: parent.width
                                    label: "Composite on close"
                                    hint: "// @overlay — keep Hyprland's own fade underneath instead of " +
                                          "replacing it. Right for a tint or wipe; wrong for a dissolve that " +
                                          "drives its own alpha."

                                    Row {
                                        spacing: Theme.gap

                                        Toggle {
                                            anchors.verticalCenter: parent.verticalCenter
                                            checked: page.selected ? page.selected.overlay : false
                                            onToggled: {
                                                App.run("shader.setOverlay", {
                                                    path: page.selectedPath,
                                                    overlay: checked
                                                })
                                            }
                                        }

                                        PillButton {
                                            anchors.verticalCenter: parent.verticalCenter
                                            visible: page.selected && page.selected.overlayStaged === true
                                            compact: true
                                            text: "Revert"
                                            tooltip: "Back to what the file says"
                                            onClicked: page.revert("overlay")
                                        }
                                    }
                                }
                            }

                            // --- parameters ---
                            Card {
                                Layout.fillWidth: true
                                visible: page.selected !== null
                                title: "Parameters"
                                subtitle: page.selected && page.selected.params.length > 0
                                          ? "These are const declarations in the file. Moving one stages a " +
                                            "change; nothing is written until you save the shader, and each " +
                                            "value can go back on its own before you do."
                                          : ""

                                Text {
                                    width: parent.width
                                    visible: page.selected && page.selected.params.length === 0
                                    wrapMode: Text.WordWrap
                                    color: Theme.muted
                                    font.pixelSize: Theme.fontSize
                                    text: "Nothing tunable found. Add a const with a plain number — " +
                                          "const float DIM = 0.6; — and it shows up here. A " +
                                          "// @param 0 1 0.01 \"Dim amount\" comment above it gives it a " +
                                          "proper range and label."
                                }

                                Repeater {
                                    model: page.selected ? page.selected.params : []

                                    ParamEditor {
                                        required property var modelData
                                        width: parent.width
                                        param: modelData
                                        shaderPath: page.selectedPath
                                    }
                                }
                            }

                            // --- uniforms ---
                            Card {
                                Layout.fillWidth: true
                                visible: page.selected && page.selected.uniforms.length > 0
                                title: "Uniforms it declares"
                                subtitle: "The plugin fills these in every frame."

                                Flow {
                                    width: parent.width
                                    spacing: Theme.gapSmall

                                    Repeater {
                                        model: page.selected ? page.selected.uniforms : []

                                        Badge {
                                            required property string modelData
                                            text: modelData
                                            tint: Theme.accent
                                        }
                                    }
                                }

                                Text {
                                    width: parent.width
                                    visible: page.selected && page.selected.isMotionDriven
                                    wrapMode: Text.WordWrap
                                    color: Theme.warn
                                    font.pixelSize: Theme.fontSizeSmall
                                    text: "Motion uniforms read zero on a layer surface, so pointing this " +
                                          "shader at a layer namespace does nothing at all — silently. " +
                                          "Drive it from progress in a layer animation instead."
                                }
                            }

                            // --- source ---
                            Card {
                                Layout.fillWidth: true
                                visible: page.selected !== null
                                title: "Source"

                                PillButton {
                                    text: page.showSource ? "Hide source" : "Show source"
                                    onClicked: page.showSource = !page.showSource
                                }

                                Rectangle {
                                    width: parent.width
                                    height: Math.min(420, sourceText.implicitHeight + Theme.gap * 2)
                                    visible: page.showSource
                                    color: Theme.bgAlt
                                    radius: Theme.radiusSmall
                                    border.width: 1
                                    border.color: Theme.border

                                    ScrollView {
                                        anchors.fill: parent
                                        anchors.margins: Theme.gap
                                        clip: true

                                        Text {
                                            id: sourceText
                                            text: page.showSource && page.selectedPath !== ""
                                                  ? App.query("shaderSource", { path: page.selectedPath })
                                                  : ""
                                            color: Theme.fgDim
                                            font.family: Theme.monoFamily
                                            font.pixelSize: Theme.fontSizeSmall
                                            textFormat: Text.PlainText
                                        }
                                    }
                                }
                            }

                            Item {
                                Layout.preferredHeight: Theme.pad
                            }
                        }
                    }

                    ShaderPreview {
                        shaderPath: page.selectedPath
                        shaderName: page.selected ? page.selected.name : ""

                        Layout.fillWidth: !page.sideBySide
                        Layout.fillHeight: page.sideBySide
                        Layout.preferredWidth: page.sideBySide ? 380 : -1
                        Layout.preferredHeight: page.sideBySide ? -1 : 300
                        Layout.minimumWidth: page.sideBySide ? 300 : 0
                    }
                }

                // --- what is staged, and the two things to do about it ---------
                //
                // Pinned below the scroll area rather than inside it: the value you
                // just moved may be far down a long list of parameters, and a save
                // button you have to scroll back up to find is a save button people
                // forget to press.
                Rectangle {
                    Layout.fillWidth: true
                    Layout.preferredHeight: Theme.rowHeight + Theme.pad
                    visible: page.selectedDirty
                    color: Theme.surface
                    radius: Theme.radiusSmall
                    border.width: 1
                    border.color: Theme.wash(Theme.warn, 0.55)

                    RowLayout {
                        anchors.fill: parent
                        anchors.leftMargin: Theme.pad
                        anchors.rightMargin: Theme.gap
                        spacing: Theme.gap

                        Text {
                            Layout.fillWidth: true
                            text: App.plural(page.selectedStaged, "change") + " not written to "
                                  + App.fileName(page.selectedPath)
                            color: Theme.fg
                            font.pixelSize: Theme.fontSize
                            elide: Text.ElideRight
                        }

                        PillButton {
                            text: "Revert all"
                            tooltip: "Put every value in this shader back to what the file says"
                            onClicked: App.confirm(
                                "Revert " + App.fileName(page.selectedPath) + "?",
                                App.plural(page.selectedStaged, "unsaved change")
                                + " will be thrown away, and every value goes back to what the "
                                + "file on disk says.",
                                "Revert", true,
                                function () { page.revert("all") })
                        }

                        PillButton {
                            text: "Save shader"
                            primary: true
                            tooltip: "Write these values into " + App.fileName(page.selectedPath)
                                     + ". The plugin reloads it as soon as the file changes."
                            onClicked: App.run("shader.save", { path: page.selectedPath })
                        }
                    }
                }
            }
        }
    }
}
