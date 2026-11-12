import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import QtQuick.Dialogs
import dev.hyprcustard.gui

// Look rules: the look changed for some windows, layers or workspaces only.
//
// The app's own rules are listed first and edited on the right; like every
// other look change they are on the desktop the moment they are made, and
// written into the look block on Save. Below them are the rules the user
// wrote by hand, which can be moved here to be edited — the original is
// commented out on the next save.
Item {
    id: page

    readonly property var info: App.lookRules
    readonly property var rules: page.info.rules || []
    readonly property var hand: page.info.hand || []
    readonly property var catalog: page.info.catalog || ({})
    readonly property bool live: App.hyprlandRunning && App.settings.live_look_edits !== false

    // "lr3" for one of the app's rules, "hand:<n>" for one of the user's.
    property string selectedId: ""

    readonly property var selected: {
        for (var i = 0; i < page.rules.length; ++i)
            if (page.rules[i].id === page.selectedId)
                return page.rules[i]
        return null
    }
    readonly property var selectedHand: {
        if (page.selectedId.indexOf("hand:") !== 0)
            return null
        return page.hand[parseInt(page.selectedId.substring(5), 10)] || null
    }

    // The selected rule's match and effect keys, as strings: a string
    // property only signals when it really changes, so the rows built from
    // it survive state pushes — a slider is not rebuilt mid-drag, nor a
    // field mid-typing. A window rule always offers its class.
    readonly property string matchKeys: {
        if (!page.selected)
            return ""
        var keys = page.selected.matches.map(function (m) { return m.key })
        if (page.selected.kind === "window" && keys.indexOf("class") < 0)
            keys.unshift("class")
        return keys.join("|")
    }
    readonly property string effectKeys: page.selected
                                         ? page.selected.effects.map(function (e) { return e.key }).join("|")
                                         : ""

    // A match field as set on the rule, or unset with an empty value.
    function matchInfo(key) {
        if (!page.selected)
            return { key: key, label: "", kind: "text", value: "" }
        for (var i = 0; i < page.selected.matches.length; ++i)
            if (page.selected.matches[i].key === key)
                return page.selected.matches[i]
        var all = page.fieldsOf(page.selected.kind, "matches")
        for (var j = 0; j < all.length; ++j)
            if (all[j].key === key)
                return { key: key, label: all[j].label, hint: all[j].hint, kind: all[j].kind, value: "", unset: true }
        return { key: key, label: key, kind: "text", value: "" }
    }

    readonly property var kindLabels: ({ window: "Windows", layer: "Layers", workspace: "Workspaces" })
    readonly property var kindOne: ({ window: "window", layer: "layer", workspace: "workspace" })

    // Rows for the list: a heading before each kind present, the user's own
    // rules last.
    readonly property var listRows: {
        var out = []
        var kinds = ["window", "layer", "workspace"]
        for (var k = 0; k < kinds.length; ++k) {
            var first = true
            for (var i = 0; i < page.rules.length; ++i) {
                if (page.rules[i].kind !== kinds[k])
                    continue
                if (first)
                    out.push({ heading: page.kindLabels[kinds[k]] })
                first = false
                out.push({ rule: page.rules[i] })
            }
        }
        if (page.hand.length > 0) {
            out.push({ heading: "In your hyprland.lua" })
            for (var h = 0; h < page.hand.length; ++h)
                out.push({ hand: page.hand[h], handIndex: h })
        }
        return out
    }

    function add(kind, extra) {
        var payload = { kind: kind }
        for (var k in extra)
            payload[k] = extra[k]
        var before = page.rules.length
        App.run("rule.addLook", payload)
        if (page.rules.length > before)
            page.selectedId = page.rules[page.rules.length - 1].id
    }

    function setRule(changes) {
        if (!page.selected)
            return
        var payload = { id: page.selected.id }
        for (var k in changes)
            payload[k] = changes[k]
        App.run("rule.setLook", payload)
    }

    function setMatch(key, value) {
        var m = {}
        m[key] = value
        page.setRule({ matches: m })
    }

    function setEffect(key, value) {
        var e = {}
        e[key] = value
        page.setRule({ effects: e })
    }

    // What the editor's rows and pickers need.
    function fieldsOf(kind, which) {
        return (page.catalog[kind] || {})[which] || []
    }
    function unused(kind, which, have) {
        var taken = {}
        for (var i = 0; i < have.length; ++i)
            taken[have[i].key] = true
        return page.fieldsOf(kind, which).filter(function (f) { return !taken[f.key] })
    }

    Connections {
        target: App
        function onLookRulesChanged() {
            if (page.selectedId !== "" && page.selected === null && page.selectedHand === null)
                page.selectedId = page.rules.length > 0 ? page.rules[page.rules.length - 1].id : ""
        }
    }

    // --- the host LookOptionRow edits effects through -------------------------

    QtObject {
        id: editHost

        readonly property var byKey: {
            var m = ({})
            var effects = page.selected ? page.selected.effects : []
            for (var i = 0; i < effects.length; ++i)
                m[effects[i].key] = effects[i]
            return m
        }
        property var pending: null

        function parentOf(key) {
            return ""
        }
        function tryLive(key, value) {
            if (!page.live || !page.selected)
                return
            editHost.pending = { key: "rule:" + page.selected.id + ":" + key, value: value }
            if (!tryTimer.running)
                tryTimer.start()
        }
        function set(key, value) {
            tryTimer.stop()
            editHost.pending = null
            page.setEffect(key, value)
        }
        function pickColor(initial, pick) {
            colorDialog.picking = false
            colorDialog.pickFn = pick
            colorDialog.selectedColor = initial
            colorDialog.picking = true
            colorDialog.open()
        }
        function hyprHex(value) {
            return App.hyprHex(value)
        }
        function gradientText(value) {
            return App.gradientText(value)
        }
        function parseGradient(text) {
            return App.parseGradient(text)
        }
    }

    // A drag sends a value per step; only the latest each frame goes on.
    Timer {
        id: tryTimer
        interval: 16
        onTriggered: {
            if (editHost.pending === null)
                return
            var p = editHost.pending
            editHost.pending = null
            App.lookTry(p.key, p.value)
        }
    }

    ColorDialog {
        id: colorDialog

        property var pickFn: null
        property bool picking: false

        title: "Pick a color"
        options: ColorDialog.ShowAlphaChannel
        onSelectedColorChanged: {
            if (colorDialog.picking && colorDialog.pickFn)
                colorDialog.pickFn(App.argb(colorDialog.selectedColor), false)
        }
        onAccepted: {
            colorDialog.picking = false
            if (colorDialog.pickFn)
                colorDialog.pickFn(App.argb(colorDialog.selectedColor), true)
        }
        onRejected: {
            colorDialog.picking = false
            if (page.live)
                App.run("look.resync", {})
        }
    }

    RowLayout {
        anchors.fill: parent
        spacing: Theme.pad

        // ------------------------------------------------------------------
        // The list
        // ------------------------------------------------------------------
        ColumnLayout {
            Layout.preferredWidth: Theme.listWidth
            Layout.minimumWidth: Theme.listWidth
            Layout.maximumWidth: Theme.listWidth
            Layout.fillHeight: true
            spacing: Theme.gap

            Text {
                text: "Add a rule for"
                color: Theme.fgDim
                font.pixelSize: Theme.fontSize
            }

            RowLayout {
                Layout.fillWidth: true
                Layout.maximumWidth: Theme.listWidth
                spacing: Theme.gapSmall

                PillButton {
                    Layout.fillWidth: true
                    text: "Windows"
                    compact: true
                    tooltip: "A look for some windows: an app, dialogs, tiled or floating ones."
                    onClicked: page.add("window", {})
                }
                PillButton {
                    Layout.fillWidth: true
                    text: "A layer"
                    compact: true
                    tooltip: "A look for a bar, launcher or notifications."
                    onClicked: page.add("layer", { matches: { namespace: "" } })
                }
                PillButton {
                    Layout.fillWidth: true
                    text: "Workspaces"
                    compact: true
                    tooltip: "Gaps and borders on some workspaces."
                    onClicked: page.add("workspace", { matches: { workspace: "" } })
                }
            }

            Dropdown {
                id: common
                Layout.fillWidth: true

                // Ready-made rules. Each is an ordinary rule once added, to
                // change or delete like any other.
                readonly property var presets: [
                    { label: "No gaps when there is only one window", add: function () {
                        var none = [0, 0, 0, 0]
                        page.add("workspace", { label: "One window: no gaps", matches: { workspace: "w[tv1]" },
                                                effects: { gaps_out: none, gaps_in: none } })
                        page.add("workspace", { label: "Fullscreen: no gaps", matches: { workspace: "f[1]" },
                                                effects: { gaps_out: none, gaps_in: none } })
                    } },
                    { label: "No border or rounding when there is only one window", add: function () {
                        page.add("window", { label: "Alone: no border", matches: { float: false, workspace: "w[tv1]" },
                                             effects: { border_size: 0, rounding: 0 } })
                    } },
                    { label: "No shadow on tiled windows", add: function () {
                        page.add("window", { label: "Tiled: no shadow", matches: { float: false },
                                             effects: { no_shadow: true } })
                    } },
                    { label: "Blur behind waybar", add: function () {
                        page.add("layer", { label: "Bar blur", matches: { namespace: "waybar" },
                                            effects: { blur: true, ignore_alpha: 0.5 } })
                    } },
                    { label: "Dim and blur behind rofi", add: function () {
                        page.add("layer", { label: "Launcher", matches: { namespace: "rofi" },
                                            effects: { blur: true, dim_around: true, ignore_alpha: 0.5 } })
                    } }
                ]

                model: ["Common rules…"].concat(common.presets.map(function (p) { return p.label }))
                currentIndex: 0
                onActivated: function (i) {
                    if (i > 0)
                        common.presets[i - 1].add()
                    common.currentIndex = 0
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
                    visible: page.listRows.length === 0
                    title: "No look rules"
                    body: "Add one to change the look of one app, a bar or a workspace."
                }

                ListView {
                    id: list
                    anchors.fill: parent
                    anchors.margins: 1
                    clip: true
                    model: page.listRows
                    currentIndex: -1

                    delegate: Item {
                        id: row

                        required property var modelData
                        required property int index

                        readonly property bool isHeading: row.modelData.heading !== undefined
                        readonly property var item: row.modelData.rule || row.modelData.hand || null
                        readonly property string rowId: row.modelData.rule
                                                        ? row.modelData.rule.id
                                                        : (row.modelData.hand ? "hand:" + row.modelData.handIndex : "")
                        readonly property bool isSelected: row.rowId !== "" && row.rowId === page.selectedId

                        width: ListView.view.width
                        height: row.isHeading ? 30 : 62

                        Text {
                            visible: row.isHeading
                            anchors.left: parent.left
                            anchors.leftMargin: Theme.gap + 4
                            anchors.bottom: parent.bottom
                            anchors.bottomMargin: 5
                            text: (row.modelData.heading || "").toUpperCase()
                            color: Theme.muted
                            font.pixelSize: Theme.fontSizeSmall
                            font.weight: Font.DemiBold
                            font.letterSpacing: 0.8
                        }

                        Rectangle {
                            visible: !row.isHeading
                            anchors.fill: parent
                            color: row.isSelected ? Theme.selection
                                                  : (hover.hovered ? Theme.surfaceHi : "transparent")

                            HoverHandler {
                                id: hover
                            }
                            MouseArea {
                                anchors.fill: parent
                                onClicked: page.selectedId = row.rowId
                            }
                            Rectangle {
                                width: 3
                                height: parent.height
                                color: Theme.accent
                                visible: row.isSelected
                            }

                            ColumnLayout {
                                anchors.fill: parent
                                anchors.margins: Theme.gap
                                anchors.leftMargin: Theme.gap + 4
                                spacing: 2

                                RowLayout {
                                    Layout.fillWidth: true
                                    spacing: Theme.gapSmall

                                    FitText {
                                        Layout.fillWidth: true
                                        text: row.item ? (row.modelData.rule
                                                          ? (row.item.label || row.item.target)
                                                          : (row.item.name || row.item.target || "A "
                                                             + page.kindOne[row.item.kind] + " rule"))
                                                       : ""
                                        color: row.modelData.rule && !row.item.enabled ? Theme.muted : Theme.fg
                                        font.pixelSize: Theme.fontSize
                                        font.weight: Font.DemiBold
                                        elide: Text.ElideRight
                                    }
                                    Badge {
                                        visible: row.modelData.rule !== undefined && row.item !== null
                                                 && !row.item.enabled
                                        text: "off"
                                        tint: Theme.muted
                                    }
                                    Badge {
                                        visible: row.modelData.rule !== undefined && row.item !== null
                                                 && row.item.enabled && !!row.item.incomplete
                                        text: "unfinished"
                                        tint: Theme.warn
                                    }
                                    Badge {
                                        visible: row.modelData.hand !== undefined && row.item !== null
                                        text: row.item && row.item.moving ? "moving here"
                                                                          : page.kindOne[row.item ? row.item.kind : "window"]
                                        tint: row.item && row.item.moving ? Theme.warn : Theme.muted
                                    }
                                }
                                FitText {
                                    Layout.fillWidth: true
                                    text: row.item ? (row.item.summary || (row.modelData.hand
                                                                          ? "Not readable here"
                                                                          : "Changes nothing yet"))
                                                   : ""
                                    color: Theme.muted
                                    font.pixelSize: Theme.fontSizeSmall
                                    elide: Text.ElideRight
                                }
                            }
                        }
                    }

                    ScrollBar.vertical: ScrollBar {}
                }
            }
        }

        // ------------------------------------------------------------------
        // The editor
        // ------------------------------------------------------------------
        Item {
            Layout.fillWidth: true
            Layout.fillHeight: true

            // Nothing selected: what the page is for, and a common one to add.
            ScrollView {
                id: introScroll
                anchors.fill: parent
                visible: page.selected === null && page.selectedHand === null
                clip: true
                contentWidth: availableWidth

                ColumnLayout {
                    width: Math.min(introScroll.availableWidth, Theme.contentMax)
                    x: Math.round((introScroll.availableWidth - width) / 2)
                    spacing: Theme.pad

                    Card {
                        Layout.fillWidth: true
                        title: "Look rules"
                        subtitle: "The Look page sets how everything looks. A rule here changes that for "
                                  + "some things only: one app see-through, no shadow on tiled windows, "
                                  + "blur behind the bar, no gaps when a workspace has one window. "
                                  + (page.live ? "Changes show on the desktop straight away; Save writes them "
                                                 + "to hyprland.lua."
                                               : "Changes show on the desktop when you save.")

                        Text {
                            width: parent.width
                            wrapMode: Text.WordWrap
                            text: "Add one with the buttons on the left, or pick a ready-made one from "
                                  + "Common rules. Each is an ordinary rule once added."
                            color: Theme.fgDim
                            font.pixelSize: Theme.fontSize
                        }

                        Text {
                            width: parent.width
                            wrapMode: Text.WordWrap
                            visible: page.hand.length > 0
                            text: "Your hyprland.lua already has " + App.plural(page.hand.length, "rule")
                                  + " like this, listed on the left. They keep working as they are; "
                                  + "move one here to edit it in the app."
                            color: Theme.fgDim
                            font.pixelSize: Theme.fontSize
                        }
                    }

                }
            }

            // One of the user's own rules.
            ScrollView {
                id: handScroll
                anchors.fill: parent
                visible: page.selectedHand !== null
                clip: true
                contentWidth: availableWidth

                ColumnLayout {
                    width: Math.min(handScroll.availableWidth, Theme.contentMax)
                    x: Math.round((handScroll.availableWidth - width) / 2)
                    spacing: Theme.pad

                    Card {
                        Layout.fillWidth: true
                        title: page.selectedHand ? (page.selectedHand.name || "A "
                                                    + page.kindOne[page.selectedHand.kind] + " rule") : ""
                        subtitle: page.selectedHand && page.selectedHand.moving
                                  ? "Moving here. Save comments this out in hyprland.lua; Discard "
                                    + "changes your mind."
                                  : "Written by you in hyprland.lua. It works as it is. Moving it here "
                                    + "lets the app edit it; your original is commented out when you "
                                    + "save, with a backup taken first."

                        Text {
                            width: parent.width
                            visible: page.selectedHand !== null && page.selectedHand.target !== ""
                            text: page.selectedHand ? "For " + page.selectedHand.target + " — "
                                                      + page.selectedHand.summary : ""
                            wrapMode: Text.WordWrap
                            color: Theme.fgDim
                            font.pixelSize: Theme.fontSize
                        }

                        Rectangle {
                            width: parent.width
                            height: handText.implicitHeight + Theme.gap * 2
                            radius: Theme.radiusSmall
                            color: Theme.bgAlt
                            border.width: 1
                            border.color: Theme.border

                            Text {
                                id: handText
                                x: Theme.gap
                                y: Theme.gap
                                width: parent.width - Theme.gap * 2
                                text: page.selectedHand ? page.selectedHand.text : ""
                                font.family: "monospace"
                                font.pixelSize: Theme.fontSizeSmall
                                color: Theme.fg
                                wrapMode: Text.WrapAnywhere
                            }
                        }

                        Row {
                            spacing: Theme.gap

                            PillButton {
                                text: "Move here"
                                primary: true
                                visible: page.selectedHand !== null && page.selectedHand.movable
                                enabled: page.selectedHand !== null && !page.selectedHand.moving
                                onClicked: {
                                    var before = page.rules.length
                                    App.run("rule.adopt", { text: page.selectedHand.text })
                                    if (page.rules.length > before)
                                        page.selectedId = page.rules[page.rules.length - 1].id
                                }
                            }
                            Text {
                                visible: page.selectedHand !== null && !page.selectedHand.movable
                                anchors.verticalCenter: parent.verticalCenter
                                text: page.selectedHand ? "Can't be moved: " + page.selectedHand.why : ""
                                color: Theme.muted
                                font.pixelSize: Theme.fontSizeSmall
                            }
                        }
                    }
                }
            }

            // One of the app's rules.
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

                    Card {
                        Layout.fillWidth: true
                        title: page.selected ? "For " + page.selected.target : ""
                        subtitle: page.selected && page.selected.incomplete
                                  ? page.selected.incomplete + "."
                                  : (page.selected && page.selected.kind === "window"
                                     ? "Every field set must match. Text is a regular expression."
                                     : "")

                        FieldRow {
                            width: parent.width
                            label: "On"
                            hint: "A rule switched off is kept here but not written."

                            Toggle {
                                checked: page.selected ? page.selected.enabled : true
                                onToggled: page.setRule({ enabled: checked })
                            }
                        }

                        FieldRow {
                            width: parent.width
                            label: "Name"
                            hint: "Yours, written as a comment above the rule."

                            LineEdit {
                                width: parent.width
                                text: page.selected ? page.selected.label : ""
                                placeholderText: "Terminal glass"
                                onCommit: function (v) { page.setRule({ label: v }) }
                            }
                        }

                        // What it matches, a row per field set.
                        Repeater {
                            model: page.matchKeys === "" ? [] : page.matchKeys.split("|")

                            FieldRow {
                                id: matchRow

                                required property var modelData
                                readonly property var field: page.matchInfo(matchRow.key)
                                readonly property string key: modelData

                                width: parent.width
                                label: matchRow.field.label
                                hint: matchRow.field.hint || ""

                                RowLayout {
                                    width: parent.width
                                    spacing: Theme.gapSmall

                                    LineEdit {
                                        id: matchText
                                        Layout.fillWidth: true
                                        visible: matchRow.field.kind === "text"
                                        mono: true
                                        text: matchRow.field.kind === "text" ? matchRow.field.value : ""
                                        onCommit: function (v) { page.setMatch(matchRow.key, v) }
                                    }
                                    // Suggestions from what is on screen.
                                    Dropdown {
                                        id: suggest
                                        readonly property var options: {
                                            var k = matchRow.key
                                            if (k === "class" || k === "initial_class")
                                                return App.liveClasses
                                            if (k === "namespace")
                                                return App.liveNamespaces
                                            if (k === "workspace" && page.selected && page.selected.kind === "workspace")
                                                return ["w[tv1]", "f[1]", "s[true]", "1", "2", "3", "special"]
                                            return []
                                        }
                                        visible: suggest.options.length > 0
                                        Layout.preferredWidth: 150
                                        model: ["Pick…"].concat(suggest.options)
                                        currentIndex: 0
                                        onActivated: function (i) {
                                            if (i > 0)
                                                page.setMatch(matchRow.key, suggest.options[i - 1])
                                            suggest.currentIndex = 0
                                        }
                                    }
                                    Toggle {
                                        visible: matchRow.field.kind === "bool"
                                        checked: matchRow.field.value === true
                                        text: matchRow.field.value === true ? "yes" : "no"
                                        onToggled: page.setMatch(matchRow.key, checked)
                                    }
                                    Item {
                                        Layout.fillWidth: matchRow.field.kind === "bool"
                                    }
                                    PillButton {
                                        compact: true
                                        text: "✕"
                                        visible: page.selected !== null && page.selected.kind === "window"
                                                 && matchRow.field.unset !== true
                                        tooltip: "Stop matching on this"
                                        onClicked: page.setMatch(matchRow.key, null)
                                    }
                                }
                            }
                        }

                        Dropdown {
                            id: addMatch
                            readonly property var options: page.selected && page.selected.kind === "window"
                                                           ? page.unused("window", "matches", page.selected.matches)
                                                                 .filter(function (f) { return f.key !== "class" })
                                                           : []
                            visible: addMatch.options.length > 0
                            width: 260
                            model: ["Also match on…"].concat(addMatch.options.map(function (f) { return f.label }))
                            currentIndex: 0
                            onActivated: function (i) {
                                if (i > 0) {
                                    var f = addMatch.options[i - 1]
                                    page.setMatch(f.key, f.kind === "bool" ? f.value : (f.value || ".*"))
                                }
                                addMatch.currentIndex = 0
                            }
                        }
                    }

                    Card {
                        Layout.fillWidth: true
                        title: "Changes"
                        subtitle: page.selected && page.selected.effects.length === 0
                                  ? "Add what this rule should change." : ""

                        Repeater {
                            // Keys, not objects: rows are kept through a state
                            // push, so a slider is not rebuilt mid-drag.
                            model: page.effectKeys === "" ? [] : page.effectKeys.split("|")

                            Item {
                                id: effectSlot

                                required property var modelData

                                width: parent.width
                                height: editHost.byKey[effectSlot.modelData]
                                        && editHost.byKey[effectSlot.modelData].kind === "fixed"
                                        ? fixedRow.implicitHeight : optRow.implicitHeight

                                LookOptionRow {
                                    id: optRow
                                    visible: !fixedRow.visible
                                    host: editHost
                                    key: effectSlot.modelData
                                    removeFn: function (k) { page.setEffect(k, null) }
                                }

                                // Kept from a moved rule, not editable here.
                                RowLayout {
                                    id: fixedRow
                                    width: parent.width
                                    visible: editHost.byKey[effectSlot.modelData] !== undefined
                                             && editHost.byKey[effectSlot.modelData].kind === "fixed"
                                    Text {
                                        Layout.preferredWidth: 150
                                        text: effectSlot.modelData
                                        color: Theme.fgDim
                                        font.pixelSize: Theme.fontSize
                                    }
                                    Text {
                                        Layout.fillWidth: true
                                        text: fixedRow.visible ? editHost.byKey[effectSlot.modelData].value : ""
                                        font.family: "monospace"
                                        color: Theme.fg
                                        font.pixelSize: Theme.fontSize
                                    }
                                }
                            }
                        }

                        // Kept from a moved rule and not about looks.
                        Repeater {
                            model: page.selected ? page.selected.extra : []

                            RowLayout {
                                id: extraRow

                                required property var modelData

                                width: parent.width
                                spacing: Theme.gap

                                Text {
                                    Layout.preferredWidth: 150
                                    text: extraRow.modelData.key
                                    color: Theme.fgDim
                                    font.pixelSize: Theme.fontSize
                                }
                                FitText {
                                    Layout.fillWidth: true
                                    text: extraRow.modelData.value + "   (kept as written)"
                                    font.family: "monospace"
                                    color: Theme.muted
                                    font.pixelSize: Theme.fontSize
                                    elide: Text.ElideRight
                                }
                                PillButton {
                                    compact: true
                                    text: "Remove"
                                    onClicked: page.setEffect(extraRow.modelData.key, null)
                                }
                            }
                        }

                        Dropdown {
                            id: addEffect
                            readonly property var options: page.selected
                                                           ? page.unused(page.selected.kind, "effects", page.selected.effects)
                                                           : []
                            visible: addEffect.options.length > 0
                            width: 260
                            model: ["Add a change…"].concat(addEffect.options.map(function (f) { return f.label }))
                            currentIndex: 0
                            onActivated: function (i) {
                                if (i > 0)
                                    page.setEffect(addEffect.options[i - 1].key, addEffect.options[i - 1].value)
                                addEffect.currentIndex = 0
                            }
                        }
                    }

                    Card {
                        Layout.fillWidth: true
                        title: "As written"
                        subtitle: "What Save puts in the look block."

                        Rectangle {
                            width: parent.width
                            height: luaText.implicitHeight + Theme.gap * 2
                            radius: Theme.radiusSmall
                            color: Theme.bgAlt
                            border.width: 1
                            border.color: Theme.border

                            Text {
                                id: luaText
                                x: Theme.gap
                                y: Theme.gap
                                width: parent.width - Theme.gap * 2
                                text: page.selected ? page.selected.lua : ""
                                font.family: "monospace"
                                font.pixelSize: Theme.fontSizeSmall
                                color: page.selected && page.selected.incomplete ? Theme.muted : Theme.fg
                                wrapMode: Text.WrapAnywhere
                            }
                        }

                        Row {
                            spacing: Theme.gap

                            PillButton {
                                text: "Move up"
                                compact: true
                                tooltip: "Rules further down win where two change the same thing."
                                onClicked: App.run("rule.moveLook", { id: page.selected.id, delta: -1 })
                            }
                            PillButton {
                                text: "Move down"
                                compact: true
                                onClicked: App.run("rule.moveLook", { id: page.selected.id, delta: 1 })
                            }
                            PillButton {
                                text: "Delete rule"
                                compact: true
                                danger: true
                                onClicked: App.run("rule.removeLook", { id: page.selected.id })
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
