import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import QtQuick.Dialogs
import dev.hyprcustard.gui

// Hyprland's own look: gaps, borders, rounding, opacity, shadow and blur.
//
// Every value shown is what Hyprland is running with. Anything changed here
// goes into its own block at the end of hyprland.lua on Save, which holds only
// the options changed here — the rest keeps coming from the user's own config.
// With live edits on, a change is on the desktop the moment it is made.
Item {
    id: page

    readonly property var look: App.look
    readonly property bool running: page.look.running === true
    readonly property bool liveEdits: App.settings.live_look_edits !== false
    readonly property bool live: page.running && page.liveEdits
    property string lastError: ""

    // Every option by key, rebuilt with each state push. Rows look their
    // option up here rather than being handed it by a model, so a push
    // updates them in place instead of rebuilding them in the middle of a drag.
    readonly property var byKey: {
        var m = ({})
        var opts = page.look.options || []
        for (var i = 0; i < opts.length; ++i)
            m[opts[i].key] = opts[i]
        return m
    }

    // The sections and the keys in each. Built once: the option list is fixed,
    // and a model that changed on every push would recreate every row.
    property var groups: []

    function buildGroups() {
        var opts = page.look.options || []
        if (page.groups.length > 0 || opts.length === 0)
            return
        var out = []
        var gs = page.look.groups || []
        for (var g = 0; g < gs.length; ++g) {
            var keys = []
            for (var i = 0; i < opts.length; ++i)
                if (opts[i].group === gs[g].id)
                    keys.push(opts[i].key)
            out.push({ id: gs[g].id, label: gs[g].label, keys: keys })
        }
        page.groups = out
    }

    Component.onCompleted: page.buildGroups()
    Connections {
        target: App
        function onLookChanged() {
            page.buildGroups()
        }
    }

    // The switch a row depends on: a shadow's size means nothing with
    // shadows off.
    function parentOf(key) {
        if (key === "decoration.dim_strength")
            return "decoration.dim_inactive"
        if (key.indexOf("decoration.shadow.") === 0 && key !== "decoration.shadow.enabled")
            return "decoration.shadow.enabled"
        if (key.indexOf("decoration.blur.") === 0 && key !== "decoration.blur.enabled")
            return "decoration.blur.enabled"
        if (key.indexOf("decoration.glow.") === 0 && key !== "decoration.glow.enabled")
            return "decoration.glow.enabled"
        return ""
    }

    // --- changing things ---------------------------------------------------

    // A drag sends a value per step; only the latest one in each frame is
    // worth sending on.
    property var pending: null

    Timer {
        id: tryTimer
        interval: 16
        onTriggered: {
            if (page.pending === null)
                return
            var p = page.pending
            page.pending = null
            page.lastError = App.lookTry(p.key, p.value)
        }
    }

    function tryLive(key, value) {
        if (!page.live)
            return
        page.pending = { key: key, value: value }
        if (!tryTimer.running)
            tryTimer.start()
    }

    function set(key, value) {
        tryTimer.stop()
        page.pending = null
        page.lastError = ""
        // A click that moved nothing must not start overriding the option.
        var o = page.byKey[key]
        if (o && !o.overridden && JSON.stringify(o.value) === JSON.stringify(value))
            return
        App.lookSet(key, value)
    }

    function updateSettings(changes) {
        var copy = JSON.parse(JSON.stringify(App.settings))
        for (var k in changes)
            copy[k] = changes[k]
        App.run("settings.update", { settings: copy })
    }

    // --- colours -------------------------------------------------------------

    // A QML colour as `#aarrggbb`, the form the backend deals in.
    function argb(c) {
        function h(x) {
            var s = Math.round(x * 255).toString(16)
            return s.length < 2 ? "0" + s : s
        }
        return "#" + h(c.a) + h(c.r) + h(c.g) + h(c.b)
    }

    // `#aarrggbb` the way Hyprland writes it, `rrggbbaa` — which is what the
    // text fields show, so a value can be copied in from a config.
    function hyprHex(value) {
        var s = String(value || "")
        return s.length === 9 ? s.substr(3) + s.substr(1, 2) : s.replace("#", "")
    }

    ColorDialog {
        id: colorDialog

        property var pickFn: null
        property bool picking: false

        title: "Pick a colour"
        options: ColorDialog.ShowAlphaChannel
        onSelectedColorChanged: {
            if (colorDialog.picking && colorDialog.pickFn)
                colorDialog.pickFn(page.argb(colorDialog.selectedColor), false)
        }
        onAccepted: {
            colorDialog.picking = false
            if (colorDialog.pickFn)
                colorDialog.pickFn(page.argb(colorDialog.selectedColor), true)
        }
        onRejected: {
            colorDialog.picking = false
            // Whatever was tried while the dialog was open is still on the
            // desktop; put it back to what the app holds.
            if (page.live)
                App.run("look.resync", {})
        }
    }

    // `pick(color, done)` is called with each colour tried, then once with
    // `done` set when the dialog is accepted.
    function pickColor(initial, pick) {
        colorDialog.picking = false
        colorDialog.pickFn = pick
        colorDialog.selectedColor = initial
        colorDialog.picking = true
        colorDialog.open()
    }

    // `ebdbb2ff 595959aa 45deg`, as a border gradient is written in a
    // hyprland.conf, into what the backend takes.
    function parseGradient(text) {
        var colors = []
        var angle = 0
        var parts = String(text).trim().split(/\s+/)
        for (var i = 0; i < parts.length; ++i) {
            if (parts[i] === "")
                continue
            if (/deg$/.test(parts[i]))
                angle = parseFloat(parts[i])
            else
                colors.push(parts[i])
        }
        return { colors: colors, angle: isNaN(angle) ? 0 : angle }
    }

    function gradientText(value) {
        if (!value || !value.colors)
            return ""
        var bits = []
        for (var i = 0; i < value.colors.length; ++i)
            bits.push(page.hyprHex(value.colors[i]))
        return bits.join(" ") + " " + Math.round(value.angle || 0) + "deg"
    }

    // --- the page ------------------------------------------------------------

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
                title: "Look"
                subtitle: "What Hyprland draws around your windows. Only what you change "
                          + "here is written, into its own block at the end of hyprland.lua; "
                          + "everything else stays as your config has it."

                Toggle {
                    text: "Show changes on the desktop as you make them"
                    checked: page.liveEdits
                    enabled: page.running
                    onToggled: page.updateSettings({ live_look_edits: checked })
                }

                Text {
                    width: parent.width
                    wrapMode: Text.WordWrap
                    color: page.running ? Theme.fgDim : Theme.warn
                    font.pixelSize: Theme.fontSizeSmall
                    text: {
                        if (!page.running)
                            return "Hyprland is not running, so these are Hyprland's defaults "
                                   + "rather than your values. Changes are still saved."
                        var n = page.look.count || 0
                        var s = n === 0 ? "Nothing is set here yet — every value is your own config's."
                                        : App.plural(n, "setting") + " set here."
                        if (page.look.dirty)
                            s += " Unsaved: Save writes them to hyprland.lua"
                                 + (page.liveEdits ? ", Discard puts the desktop back." : ".")
                        return s
                    }
                }

                Text {
                    width: parent.width
                    visible: page.lastError !== ""
                    wrapMode: Text.WordWrap
                    color: Theme.error
                    font.pixelSize: Theme.fontSizeSmall
                    text: "Hyprland said: " + page.lastError
                }

                Row {
                    spacing: Theme.gap

                    PillButton {
                        text: "Discard changes"
                        enabled: page.look.dirty === true
                        tooltip: "Go back to what hyprland.lua says, on the desktop too."
                        onClicked: App.run("look.discard", {})
                    }
                    PillButton {
                        text: "Clear all"
                        danger: true
                        enabled: (page.look.count || 0) > 0
                        tooltip: "Stop setting anything here. Once saved, the look block "
                                 + "is taken out of hyprland.lua."
                        onClicked: App.confirm(
                            "Clear every look setting?",
                            "Your own config's values take over again once you save.",
                            "Clear", true,
                            function () { App.run("look.resetAll", {}) })
                    }
                }
            }

            Repeater {
                model: page.groups

                Card {
                    id: groupCard
                    required property var modelData
                    Layout.fillWidth: true
                    title: groupCard.modelData.label
                    spacing: Theme.pad

                    Repeater {
                        model: groupCard.modelData.keys

                        LookOptionRow {
                            required property var modelData
                            host: page
                            key: modelData
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
