import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import dev.hyprcustard.gui

// Fishsticks & Custard, the voice assistant: install it, start it with
// Hyprland, bind its keys, and edit its config.
//
// Start and keys are written to their own block in hyprland.lua as soon as
// they change. The assistant's settings are staged here and written to its
// config.toml on Save; fscd reads that file only when it starts, so Save
// offers to restart it.
Item {
    id: page

    readonly property var fsc: App.fsc
    readonly property var facts: page.fsc.facts || null
    // The agent the saved config chooses; the page's choice takes effect on Save.
    readonly property var agent: page.facts ? page.facts.agent : null
    readonly property var settings: page.fsc.settings || ({ keys: [] })
    readonly property bool installed: page.facts !== null && !!page.facts.installed
    readonly property bool busy: App.hyprpmBusy
    readonly property bool ours: App.jobOwner === "fsc"

    // Lines in the user's config that start fscd or bind its keys, which the
    // block would run a second time.
    readonly property var handLines: (page.fsc.userLines || []).filter(function (l) {
        return l.does.kind !== "other"
    })
    readonly property bool blocked: !page.fsc.managed && page.handLines.length > 0

    // Settings changed here and not yet written: name -> value, or null to
    // go back to FSC's default. Staged in the backend, so the header's
    // unsaved badge and the quit check count them too.
    readonly property var pending: page.fsc.pending || ({})
    readonly property int pendingCount: Object.keys(page.pending).length
    property bool savedSinceRestart: false

    property int logLines: 0
    property bool lastTransient: false

    Component.onCompleted: App.run("fsc.refresh")
    onVisibleChanged: if (visible) App.run("fsc.refresh")

    function setKeys(change) {
        var copy = JSON.parse(JSON.stringify(page.settings))
        change(copy)
        App.run("fsc.set", { settings: copy })
    }

    function stage(name, value) {
        App.run("fsc.stage", { name: name, value: value })
    }

    // What a field shows: a staged change, else what the file says, else
    // FSC's default.
    function shown(f) {
        if (page.pending.hasOwnProperty(f.name))
            return page.pending[f.name] === null ? f["default"] : page.pending[f.name]
        return f.value === null || f.value === undefined ? f["default"] : f.value
    }

    // The agent that answers, as the page stands: the Claude card and each
    // ACP agent's card show only while theirs is the one chosen.
    readonly property string backend: {
        var fields = page.fsc.fields || []
        for (var i = 0; i < fields.length; i++) {
            if (fields[i].name === "agent.backend")
                return page.shown(fields[i])
        }
        return "claude"
    }

    // A list field as a plain array of words. From the backend it arrives as
    // a Qt sequence, which Array.isArray does not count as an array.
    function words(v) {
        if (v === null || v === undefined || typeof v === "string" || v.length === undefined)
            return null
        var out = []
        for (var i = 0; i < v.length; i++)
            out.push(String(v[i]))
        return out
    }

    // A list field as one line.
    function listText(v) {
        var w = page.words(v)
        return w === null ? "" : w.join(", ")
    }

    function sameList(a, b) {
        var x = page.words(a)
        var y = page.words(b)
        return x !== null && y !== null && x.join("\u0000") === y.join("\u0000")
    }

    function fieldsIn(section) {
        return (page.fsc.fields || []).filter(function (f) { return f.section === section })
    }

    function clashFor(action) {
        var list = page.fsc.clashes || []
        for (var i = 0; i < list.length; ++i)
            if (list[i].action === action)
                return list[i].taken.join(", ")
        return ""
    }

    function appendLine(line, isTransient) {
        // pacman and the model download redraw progress bars in place.
        if (page.lastTransient) {
            var cut = log.text.lastIndexOf("\n")
            log.text = cut >= 0 ? log.text.substring(0, cut) : ""
            page.logLines = Math.max(0, page.logLines - 1)
        }
        page.lastTransient = isTransient === true
        log.append(line)
        page.logLines += 1
        if (page.logLines > 4000) {
            var kept = log.text.split("\n").slice(-2000)
            log.text = kept.join("\n")
            page.logLines = kept.length
        }
        var bar = logScroll.ScrollBar.vertical
        bar.position = Math.max(0, 1 - bar.size)
    }

    Connections {
        target: App.backend

        function onHyprpmStarted(label) {
            if (!page.ours)
                return
            page.appendLine("── " + label + " ──", false)
        }
        function onHyprpmOutput(line, isTransient) {
            if (page.ours)
                page.appendLine(line, isTransient)
        }
        function onHyprpmFinished(ok, message) {
            if (!page.ours)
                return
            page.appendLine(message, false)
            App.run("fsc.refresh")
        }
    }

    PageScroll {
        id: scroll
        anchors.fill: parent

        ColumnLayout {
            width: Math.min(scroll.availableWidth, Theme.contentMax)
            x: Math.round((scroll.availableWidth - width) / 2)
            spacing: Theme.pad

            SaveNote {
                Layout.fillWidth: true
                label: "Two ways"
                text: "Start and keys are written to your Hyprland config as soon as you change them. Its settings below wait for their own Save."
            }

            // ---------------------------------------------------------------
            // What is installed
            // ---------------------------------------------------------------
            Card {
                Layout.fillWidth: true
                title: "Fishsticks & Custard"
                subtitle: "A voice assistant that answers through Claude Code or another agent. Say "
                          + "“fishsticks and custard”, press its key, or click its tray icon."

                Flow {
                    width: parent.width
                    spacing: Theme.gapSmall

                    Badge {
                        text: page.facts === null ? "checking…"
                              : (page.installed ? "installed " + page.facts.installed : "not installed")
                        tint: page.installed ? Theme.ok : Theme.muted
                    }
                    Badge {
                        visible: page.installed
                        text: page.facts && page.facts.running ? "running" : "not running"
                        tint: page.facts && page.facts.running ? Theme.ok : Theme.warn
                    }
                    Badge {
                        visible: page.installed
                        text: page.facts && page.facts.missing_models.length === 0
                              ? "speech models ready"
                              : (page.facts ? page.facts.missing_models.length + " models missing" : "")
                        tint: page.facts && page.facts.missing_models.length === 0 ? Theme.ok : Theme.warn
                    }
                    Badge {
                        visible: page.facts !== null
                        text: page.agent ? page.agent.name + (page.agent.found ? " found" : " not found") : ""
                        tint: page.agent && page.agent.found ? Theme.ok : Theme.error
                    }
                }

                Text {
                    width: parent.width
                    visible: page.agent !== null && !page.agent.found
                    text: {
                        var a = page.agent
                        if (a === null)
                            return ""
                        if (a.backend === "claude")
                            return "It talks to Claude through the Claude Code command line program, which "
                                   + "is not a pacman package. Install it from claude.com/claude-code and log in "
                                   + "once in a terminal; then set its path below if it is not called claude."
                        if (a.program === "")
                            return "A custom agent has no usual program: set its program on the Custom agent card below."
                        return "FSC answers through " + a.program + ", which was not found. Install "
                               + a.install + (a.login !== "" ? ", sign in once with " + a.login : "")
                               + ", or set its program below."
                    }
                    color: Theme.warn
                    font.pixelSize: Theme.fontSize
                    wrapMode: Text.WordWrap
                }

                Text {
                    width: parent.width
                    visible: page.facts !== null && !page.facts.pacman && !page.installed
                    text: "Installing from here uses pacman, which this system does not have. "
                          + "The project page explains building it from source."
                    color: Theme.warn
                    font.pixelSize: Theme.fontSize
                    wrapMode: Text.WordWrap
                }

                Text {
                    width: parent.width
                    visible: page.facts !== null && page.facts.pacman
                    text: page.installed
                          ? "Updates come with the rest of your system, from your package manager "
                            + "(sudo pacman -Syu, or your AUR helper)."
                          : "Install trusts the project's signing key (checked against its known "
                            + "fingerprint), adds its pacman repository, and installs it with a full "
                            + "system upgrade. It asks for your password."
                    color: Theme.muted
                    font.pixelSize: Theme.fontSizeSmall
                    wrapMode: Text.WordWrap
                }

                Flow {
                    width: parent.width
                    spacing: Theme.gapSmall

                    PillButton {
                        visible: !page.installed
                        text: "Install"
                        primary: true
                        enabled: !page.busy && page.facts !== null && page.facts.pacman
                        onClicked: App.fscRun("install")
                    }
                    PillButton {
                        text: "Download models"
                        primary: page.installed && page.facts.missing_models.length > 0
                        enabled: !page.busy && page.installed
                        tooltip: page.facts ? "Into " + page.facts.models_dir + " (about 640 MB the first time)" : ""
                        onClicked: App.fscRun("models")
                    }
                    PillButton {
                        text: "Test speech"
                        enabled: !page.busy && page.installed && page.facts.missing_models.length === 0
                        tooltip: "Piper speaks a sentence and whisper transcribes it back"
                        onClicked: App.fscRun("selftest")
                    }
                    PillButton {
                        text: page.facts && page.facts.running ? "Restart" : "Start"
                        enabled: page.installed
                        tooltip: "fscd reads its settings when it starts"
                        onClicked: {
                            page.savedSinceRestart = false
                            App.run("fsc.restart")
                        }
                    }
                    PillButton {
                        text: "Check again"
                        onClicked: App.run("fsc.refresh")
                    }
                    PillButton {
                        text: "Project page"
                        onClicked: Qt.openUrlExternally(page.fsc.homePage)
                    }
                    PillButton {
                        text: "Remove"
                        danger: true
                        visible: page.installed && page.facts.pacman
                        enabled: !page.busy
                        onClicked: App.confirm("Remove Fishsticks & Custard?",
                                               "pacman removes the package. Its config, memory and "
                                               + "speech models in your home folder stay, and so do "
                                               + "the repository and key.",
                                               "Remove", true,
                                               function () { App.fscRun("remove") })
                    }
                }

                // What a job is doing.
                Rectangle {
                    width: parent.width
                    height: 200
                    visible: page.logLines > 0 || (page.busy && page.ours)
                    color: Theme.bgAlt
                    radius: Theme.radiusSmall

                    ScrollView {
                        id: logScroll
                        anchors.fill: parent
                        anchors.margins: 1
                        clip: true

                        TextArea {
                            id: log
                            readOnly: true
                            selectByMouse: true
                            wrapMode: Text.NoWrap
                            color: Theme.fgDim
                            selectionColor: Theme.accent
                            selectedTextColor: Theme.onAccent
                            font.family: Theme.monoFamily
                            font.pixelSize: Theme.fontSizeSmall
                            background: null
                        }
                    }

                    PillButton {
                        anchors.right: parent.right
                        anchors.top: parent.top
                        anchors.margins: Theme.gapSmall
                        compact: true
                        text: page.busy && page.ours ? "Cancel" : "Clear"
                        danger: page.busy && page.ours
                        onClicked: {
                            if (page.busy && page.ours) {
                                App.hyprpmCancel()
                            } else {
                                log.text = ""
                                page.logLines = 0
                                page.lastTransient = false
                            }
                        }
                    }
                }
            }

            // ---------------------------------------------------------------
            // Start and keys
            // ---------------------------------------------------------------
            Card {
                Layout.fillWidth: true
                title: "Start and keys"
                subtitle: page.fsc.managed && page.fsc.block
                          ? "Written to their own block in " + App.configPathDisplay
                            + ", from line " + page.fsc.block.line + ", as soon as they change."
                          : "Written to their own block at the end of " + App.configPathDisplay
                            + " as soon as they change."

                // Lines the user wrote: the block would run them twice.
                Column {
                    width: parent.width
                    visible: page.blocked
                    spacing: Theme.gapSmall

                    Text {
                        width: parent.width
                        text: "Your config already runs it. Take these lines over and they are "
                              + "commented out, with the block doing the same; Give them back "
                              + "puts them back exactly."
                        color: Theme.warn
                        font.pixelSize: Theme.fontSize
                        wrapMode: Text.WordWrap
                    }
                    Repeater {
                        model: page.handLines

                        Text {
                            required property var modelData
                            width: parent.width
                            text: "line " + modelData.number + ":  " + modelData.text
                                  + (modelData.movable ? "" : "   (shares its line with other code)")
                            color: modelData.movable ? Theme.fgDim : Theme.error
                            font.family: Theme.monoFamily
                            font.pixelSize: Theme.fontSizeSmall
                            elide: Text.ElideRight
                        }
                    }
                    PillButton {
                        text: "Take them over"
                        primary: true
                        onClicked: App.run("fsc.handOver")
                    }
                }

                Toggle {
                    text: "Start fscd with Hyprland"
                    enabled: !page.blocked
                    checked: page.settings.autostart === true
                    onToggled: page.setKeys(function (s) { s.autostart = checked })
                }
                Toggle {
                    text: "Float the window at the top center, on every workspace"
                    enabled: !page.blocked
                    checked: page.settings.window_rule === true
                    onToggled: page.setKeys(function (s) { s.window_rule = checked })
                }

                Repeater {
                    model: page.fsc.actions || []

                    FieldRow {
                        id: keyRow

                        required property var modelData
                        required property int index
                        readonly property var entry: page.settings.keys[keyRow.index] || ({})
                        readonly property string clash: page.clashFor(keyRow.modelData.action)

                        width: parent.width
                        label: keyRow.modelData.label
                        hint: keyRow.clash !== "" ? "Also bound in " + App.configPathDisplay + ", " + keyRow.clash : ""
                        enabled: !page.blocked

                        RowLayout {
                            width: parent.width
                            spacing: Theme.gap

                            Toggle {
                                checked: keyRow.entry.enabled === true
                                enabled: (keyRow.entry.key || "") !== ""
                                onToggled: page.setKeys(function (s) {
                                    s.keys[keyRow.index].enabled = checked
                                })
                            }
                            LineEdit {
                                Layout.fillWidth: true
                                mono: true
                                text: keyRow.entry.key || ""
                                placeholderText: "Not bound"
                                onCommit: function (v) {
                                    var key = v.trim()
                                    if (key === (keyRow.entry.key || ""))
                                        return
                                    page.setKeys(function (s) {
                                        s.keys[keyRow.index].key = key
                                        s.keys[keyRow.index].enabled = key !== ""
                                    })
                                }
                            }
                        }
                    }
                }

                RowLayout {
                    width: parent.width
                    visible: (page.fsc.moved || []).length > 0
                    spacing: Theme.gap

                    Text {
                        Layout.fillWidth: true
                        text: App.plural((page.fsc.moved || []).length, "line")
                              + " of your own are commented out in its place."
                        color: Theme.muted
                        font.pixelSize: Theme.fontSizeSmall
                        wrapMode: Text.WordWrap
                    }
                    PillButton {
                        text: "Give them back"
                        onClicked: App.confirm("Give your lines back?",
                                               "The block is taken out and your own lines are "
                                               + "uncommented, exactly as they were.",
                                               "Give them back", false,
                                               function () { App.run("fsc.giveBack") })
                    }
                }
            }

            // ---------------------------------------------------------------
            // The assistant's own settings
            // ---------------------------------------------------------------
            Card {
                Layout.fillWidth: true
                title: "Its settings"
                subtitle: "From " + (page.fsc.configPath || "~/.config/fsc/config.toml")
                          + ". Only what you change is written; comments in the file stay. "
                          + "Empty a field to go back to the default."

                Text {
                    width: parent.width
                    visible: !!page.fsc.configError
                    text: page.fsc.configError || ""
                    color: Theme.error
                    font.pixelSize: Theme.fontSize
                    wrapMode: Text.WordWrap
                }

                RowLayout {
                    width: parent.width
                    spacing: Theme.gapSmall

                    Text {
                        Layout.fillWidth: true
                        text: page.pendingCount > 0
                              ? App.plural(page.pendingCount, "change") + " not saved yet"
                              : (page.savedSinceRestart ? "Saved. Restart fscd to use them." : "")
                        color: page.pendingCount > 0 ? Theme.warn : Theme.muted
                        font.pixelSize: Theme.fontSize
                    }
                    PillButton {
                        text: "Revert"
                        enabled: page.pendingCount > 0
                        onClicked: App.run("fsc.revert")
                    }
                    PillButton {
                        text: "Save"
                        primary: true
                        enabled: page.pendingCount > 0 && !page.fsc.configError
                        onClicked: {
                            App.run("fsc.config")
                            // Staged changes stay when the write fails.
                            page.savedSinceRestart = page.pendingCount === 0
                            App.run("fsc.refresh")
                        }
                    }
                    PillButton {
                        text: "Restart fscd"
                        visible: page.savedSinceRestart && page.installed
                        onClicked: {
                            page.savedSinceRestart = false
                            App.run("fsc.restart")
                        }
                    }
                }
            }

            Repeater {
                model: [
                    { section: "agent", title: "Agent" },
                    { section: "claude", title: "Claude", agent: "claude" },
                    { section: "codex", title: "Codex", agent: "codex" },
                    { section: "antigravity", title: "Antigravity", agent: "antigravity" },
                    { section: "opencode", title: "OpenCode", agent: "opencode" },
                    { section: "acp", title: "Custom agent", agent: "acp" },
                    { section: "ui", title: "Window" },
                    { section: "voice", title: "Voice" }
                ]

                Card {
                    id: sectionCard

                    required property var modelData

                    Layout.fillWidth: true
                    visible: !sectionCard.modelData.agent || sectionCard.modelData.agent === page.backend
                    title: sectionCard.modelData.title

                    Repeater {
                        model: page.fieldsIn(sectionCard.modelData.section)

                        FieldRow {
                            id: row

                            required property var modelData
                            readonly property var current: page.shown(row.modelData)
                            readonly property bool changed: page.pending.hasOwnProperty(row.modelData.name)

                            width: parent.width
                            labelWidth: 220
                            label: row.modelData.label + (row.changed ? "  •" : "")
                            hint: {
                                var h = row.modelData.hint
                                if (row.modelData.name === "voice.whisper_model" && page.facts
                                        && page.facts.whisper_models.length > 0)
                                    h += ". Downloaded: " + page.facts.whisper_models.join(", ")
                                return h
                            }

                            Item {
                                width: parent.width
                                implicitHeight: Theme.rowHeight

                                Toggle {
                                    visible: row.modelData.kind === "bool"
                                    anchors.verticalCenter: parent.verticalCenter
                                    checked: row.current === true
                                    onToggled: page.stage(row.modelData.name, checked)
                                }

                                Dropdown {
                                    visible: row.modelData.kind === "choice"
                                    width: Math.min(parent.width, 260)
                                    model: row.modelData.choices || []
                                    currentIndex: (row.modelData.choices || []).indexOf(row.current)
                                    onActivated: function (index) {
                                        page.stage(row.modelData.name, row.modelData.choices[index])
                                    }
                                }

                                LineEdit {
                                    visible: row.modelData.kind === "list"
                                    width: parent.width
                                    text: page.sameList(row.current, row.modelData["default"])
                                          ? "" : page.listText(row.current)
                                    placeholderText: page.listText(row.modelData["default"])
                                    onCommit: function (v) {
                                        var t = v.trim()
                                        var value = t === "" ? null : t.split(",").map(function (w) {
                                            return w.trim()
                                        }).filter(function (w) { return w !== "" })
                                        if (value === null ? (row.modelData.value === null && !row.changed)
                                                           : page.sameList(value, row.current))
                                            return
                                        page.stage(row.modelData.name, value)
                                    }
                                }

                                LineEdit {
                                    visible: row.modelData.kind === "text" || row.modelData.kind === "int"
                                             || row.modelData.kind === "float"
                                    width: row.modelData.kind === "text" ? parent.width : Math.min(parent.width, 160)
                                    mono: row.modelData.kind !== "text"
                                    text: row.current === null || row.current === undefined
                                          || row.current === row.modelData["default"] ? "" : String(row.current)
                                    placeholderText: String(row.modelData["default"])
                                    onCommit: function (v) {
                                        var t = v.trim()
                                        var kind = row.modelData.kind
                                        var value
                                        if (t === "")
                                            value = null
                                        else if (kind === "int")
                                            value = parseInt(t, 10)
                                        else if (kind === "float")
                                            value = parseFloat(t)
                                        else
                                            value = t
                                        if ((kind === "int" || kind === "float") && value !== null && isNaN(value))
                                            return
                                        var before = row.current
                                        if (value === null ? (row.modelData.value === null && !row.changed)
                                                           : value === before)
                                            return
                                        page.stage(row.modelData.name, value)
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
