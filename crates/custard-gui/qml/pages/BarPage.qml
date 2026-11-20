import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import dev.hyprcustard.gui

// hyprcustard's own bar: a Quickshell config the app writes and starts.
// Quickshell is the user's to install; this page only says so.
Item {
    id: page

    readonly property var bar: App.state.bar || ({})
    readonly property var choice: page.bar.settings || ({})
    readonly property var startup: page.bar.startup || ({})
    readonly property bool haveQs: !!page.bar.quickshell
    readonly property bool installed: page.bar.installed === true
    readonly property bool running: page.bar.running === true
    // Starting, stopping, restarting or removing, on a thread: the buttons
    // wait until it is done.
    readonly property string busy: page.bar.busy || ""
    readonly property var lines: page.startup.lines || []
    readonly property var moved: page.startup.moved || []
    readonly property bool stuckLines: page.lines.some(function (l) { return !l.movable })
    readonly property var families: Qt.fontFamilies()
    // The font set for the bar, kept in the list even when it is not
    // installed, so the list says what is set rather than "Follow the system".
    readonly property string savedFont: page.choice.font || ""
    readonly property bool fontMissing: page.savedFont !== "" && page.families.indexOf(page.savedFont) < 0
    readonly property var fontChoices: page.fontMissing ? [page.savedFont].concat(page.families) : page.families

    Component.onCompleted: App.run("bar.refresh")
    onVisibleChanged: if (visible) App.run("bar.refresh")

    function set(changes) {
        App.run("bar.set", changes)
    }

    function hasModule(id) {
        return (page.choice.modules || []).indexOf(id) >= 0
    }

    function setModule(id, on) {
        // Kept in the bar's own order, whatever order they were turned on in.
        var all = (page.bar.modules || []).map(function (m) { return m.id })
        var now = page.choice.modules || []
        page.set({ modules: all.filter(function (m) { return m === id ? on : now.indexOf(m) >= 0 }) })
    }

    function onMonitor(name) {
        return (page.choice.monitors || []).indexOf(name) >= 0
    }

    function setMonitor(name, on) {
        var now = (page.choice.monitors || []).filter(function (m) { return m !== name })
        if (on)
            now.push(name)
        page.set({ monitors: now })
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
                label: "Saves at once"
                text: "Changes here are saved as you make them, and a running bar follows them at "
                      + "once. Its colors come from the Colors page."
            }

            // -----------------------------------------------------------
            Card {
                Layout.fillWidth: true
                title: "hyprcustard's bar"
                subtitle: "A bar for the top or bottom of the screen with a clock, workspaces, the "
                          + "window title, media, the tray, volume, Bluetooth, network, CPU and memory. "
                          + "It runs in Quickshell, beside any Quickshell config of your own."

                Flow {
                    width: parent.width
                    spacing: Theme.gapSmall

                    Badge {
                        text: !page.bar.scanned ? "checking…"
                              : page.haveQs ? "Quickshell installed" : "Quickshell not installed"
                        tint: page.haveQs ? Theme.ok : Theme.warn
                    }
                    Badge {
                        visible: page.haveQs
                        text: page.installed ? "set up" : "not set up yet"
                        tint: page.installed ? Theme.ok : Theme.muted
                    }
                    Badge {
                        visible: page.installed
                        text: page.busy === "start" || page.busy === "restart" ? "starting…"
                              : page.busy === "stop" || page.busy === "remove" ? "stopping…"
                              : page.running ? "running" : "not running"
                        tint: page.busy !== "" ? Theme.warn : page.running ? Theme.ok : Theme.muted
                    }
                }

                Text {
                    visible: page.bar.scanned === true && !page.haveQs
                    width: parent.width
                    wrapMode: Text.WordWrap
                    text: "The bar needs Quickshell, which hyprcustard does not install for you. "
                          + "Install the quickshell package with your package manager (on Arch: "
                          + "sudo pacman -S quickshell), then come back to this page."
                    color: Theme.warn
                    font.pixelSize: Theme.fontSize
                }

                Text {
                    visible: page.installed
                    width: parent.width
                    wrapMode: Text.WordWrap
                    text: "Its files are in " + page.bar.dir + ", and it runs as "
                          + page.bar.command + ". hyprcustard rewrites them when it updates, so to "
                          + "make the bar your own, copy the folder under another name and start that one."
                    color: Theme.fgDim
                    font.pixelSize: Theme.fontSizeSmall
                }

                RowLayout {
                    spacing: Theme.gapSmall

                    // Every button keeps its place: one that does not apply
                    // now is disabled rather than hidden, so a second click
                    // never lands on a button that took another's spot.
                    PillButton {
                        primary: !page.installed
                        enabled: page.haveQs && !page.installed
                        text: "Set up the bar"
                        tooltip: "Writes the bar to " + (page.bar.dir || "") + " with the choices below "
                                 + "and your current colors. Nothing starts yet."
                        onClicked: App.run("bar.install", {})
                    }
                    PillButton {
                        primary: page.installed && !page.running
                        enabled: page.haveQs && page.installed && !page.running && page.busy === ""
                        text: "Start it"
                        onClicked: App.backend.barControl("start")
                    }
                    PillButton {
                        enabled: page.running && page.busy === ""
                        text: "Stop it"
                        onClicked: App.backend.barControl("stop")
                    }
                    PillButton {
                        enabled: page.running && page.busy === ""
                        text: "Restart it"
                        tooltip: "Changes on this page show without a restart; this is for a bar "
                                 + "that has stopped responding."
                        onClicked: App.backend.barControl("restart")
                    }
                    PillButton {
                        text: "Look again"
                        tooltip: "Check again for Quickshell, fonts, monitors and apps"
                        onClicked: App.run("bar.refresh")
                    }
                    Item {
                        Layout.fillWidth: true
                    }
                    PillButton {
                        enabled: page.installed && page.busy === ""
                        danger: true
                        text: "Remove the bar"
                        tooltip: "Stops it, takes its start out of your config (giving back any lines "
                                 + "it replaced) and deletes the files hyprcustard wrote. Your choices "
                                 + "here are kept."
                        onClicked: App.confirm("Remove the bar?",
                                               "It is stopped, its start is taken out of your config "
                                               + "(any lines it replaced come back), and the files "
                                               + "hyprcustard wrote are deleted. Your choices on this "
                                               + "page are kept.",
                                               "Remove", true,
                                               function () { App.backend.barControl("remove") })
                    }
                }
            }

            // -----------------------------------------------------------
            Card {
                Layout.fillWidth: true
                visible: page.installed
                title: "At login"
                subtitle: page.startup.managed
                          ? "The bar starts at login, from a block at the end of " + page.startup.config + "."
                          : page.lines.length > 0
                            ? page.startup.config + " starts another bar at login:"
                            : "Nothing in " + page.startup.config + " starts a bar at login."

                Repeater {
                    model: page.startup.managed ? page.moved.concat(page.lines) : page.lines
                    Text {
                        required property var modelData
                        readonly property bool isMoved: page.moved.indexOf(modelData) >= 0
                        width: parent ? parent.width : 0
                        text: (isMoved ? "commented out, line " : "line ") + modelData.number + ":  "
                              + modelData.text
                        color: isMoved ? Theme.muted : modelData.movable === false ? Theme.warn : Theme.fg
                        font.family: Theme.monoFamily
                        font.pixelSize: Theme.fontSizeSmall
                        elide: Text.ElideRight
                    }
                }

                Text {
                    visible: page.startup.managed === true && page.lines.length > 0
                    width: parent.width
                    wrapMode: Text.WordWrap
                    text: "Those lines still start another bar too, so two bars will be up after you log in."
                    color: Theme.warn
                    font.pixelSize: Theme.fontSizeSmall
                }

                Text {
                    visible: page.stuckLines
                    width: parent.width
                    wrapMode: Text.WordWrap
                    text: "The line in yellow has other code on it, so hyprcustard will not comment "
                          + "it out. Put the start command on a line of its own to hand it over."
                    color: Theme.warn
                    font.pixelSize: Theme.fontSizeSmall
                }

                Rectangle {
                    visible: page.startup.managed === true && !!page.startup.block
                    width: parent.width
                    implicitHeight: blockText.implicitHeight + 2 * Theme.gapSmall
                    radius: Theme.radiusSmall
                    color: Theme.bgAlt
                    border.width: 1
                    border.color: Theme.border

                    TextEdit {
                        id: blockText
                        x: Theme.gapSmall
                        y: Theme.gapSmall
                        width: parent.width - 2 * Theme.gapSmall
                        readOnly: true
                        selectByMouse: true
                        wrapMode: TextEdit.WrapAnywhere
                        text: page.startup.block ? page.startup.block.text : ""
                        color: Theme.fg
                        font.family: Theme.monoFamily
                        font.pixelSize: Theme.fontSizeSmall
                    }
                }

                RowLayout {
                    spacing: Theme.gapSmall

                    PillButton {
                        visible: page.lines.length > 0
                        enabled: !page.stuckLines
                        text: "Start it instead of " + (page.lines.length === 1 ? "that line" : "those lines")
                        tooltip: "Comment out the lines above (marked, so Give them back restores them) "
                                 + "and start hyprcustard's bar at login instead. A backup is made first."
                        onClicked: App.run("bar.handOver", {})
                    }
                    PillButton {
                        visible: !page.startup.managed
                        text: page.lines.length > 0 ? "Start both" : "Start it at login"
                        tooltip: "Add a block that starts the bar at login. A backup is made first."
                        onClicked: App.run("bar.login", { on: true })
                    }
                    PillButton {
                        visible: page.startup.managed === true
                        text: page.moved.length > 0 ? "Give them back" : "Don't start it at login"
                        tooltip: page.moved.length > 0
                                 ? "Take hyprcustard's block out and put your own lines back as they were"
                                 : "Take hyprcustard's block out of your config"
                        onClicked: App.run("bar.login", { on: false })
                    }
                }
            }

            // -----------------------------------------------------------
            Card {
                Layout.fillWidth: true
                title: "Where it goes"

                FieldRow {
                    width: parent.width
                    label: "Monitors"
                    hint: (page.choice.monitors || []).length === 0
                          ? "On every monitor. Pick some to show it on those only."
                          : "Only on the monitors picked."

                    Flow {
                        width: parent.width
                        spacing: Theme.gap

                        Repeater {
                            // The ones picked stay listed even while unplugged.
                            model: {
                                var all = (page.bar.monitors || []).slice()
                                ;(page.choice.monitors || []).forEach(function (m) {
                                    if (all.indexOf(m) < 0)
                                        all.push(m)
                                })
                                return all
                            }
                            Toggle {
                                required property string modelData
                                text: modelData
                                checked: page.onMonitor(modelData)
                                onToggled: page.setMonitor(modelData, checked)
                            }
                        }
                        Text {
                            visible: (page.bar.monitors || []).length === 0
                                     && (page.choice.monitors || []).length === 0
                            text: page.bar.scanned ? "No monitors found; Hyprland may not be running."
                                                   : "Looking…"
                            color: Theme.muted
                            font.pixelSize: Theme.fontSizeSmall
                        }
                    }
                }

                FieldRow {
                    width: parent.width
                    label: "Edge"
                    Dropdown {
                        width: 200
                        model: ["Top", "Bottom"]
                        currentIndex: page.choice.position === "bottom" ? 1 : 0
                        onActivated: function (index) { page.set({ position: index === 1 ? "bottom" : "top" }) }
                    }
                }

                LabeledSlider {
                    width: parent.width
                    label: "Height (px)"
                    from: 20
                    to: 80
                    stepSize: 1
                    integer: true
                    value: page.choice.height || 36
                    markValue: 36
                    onSettled: function (v) { page.set({ height: Math.round(v) }) }
                }
                LabeledSlider {
                    width: parent.width
                    label: "Space from the screen's edge (px)"
                    from: 0
                    to: 40
                    stepSize: 1
                    integer: true
                    value: page.choice.edge_margin === undefined ? 4 : page.choice.edge_margin
                    markValue: 4
                    onSettled: function (v) { page.set({ edge_margin: Math.round(v) }) }
                }
                LabeledSlider {
                    width: parent.width
                    label: "Space from the screen's sides (px)"
                    from: 0
                    to: 600
                    stepSize: 2
                    integer: true
                    value: page.choice.side_margin || 0
                    markValue: 0
                    onSettled: function (v) { page.set({ side_margin: Math.round(v) }) }
                }
                LabeledSlider {
                    width: parent.width
                    label: "Space between the bar and windows (px)"
                    from: 0
                    to: 40
                    stepSize: 1
                    integer: true
                    value: page.choice.window_gap || 0
                    markValue: 0
                    onSettled: function (v) { page.set({ window_gap: Math.round(v) }) }
                }
                LabeledSlider {
                    width: parent.width
                    label: "Background opacity"
                    from: 0
                    to: 1
                    stepSize: 0.05
                    value: page.choice.opacity === undefined ? 0.8 : page.choice.opacity
                    markValue: 0.8
                    onSettled: function (v) { page.set({ opacity: Math.round(v * 100) / 100 }) }
                }
            }

            // -----------------------------------------------------------
            Card {
                Layout.fillWidth: true
                title: "Text and icons"

                FieldRow {
                    width: parent.width
                    label: "Font"
                    hint: "Following the system uses the font Qt is given by qt6ct or your desktop."
                    Dropdown {
                        width: parent.width
                        model: ["Follow the system"].concat(page.fontChoices.map(function (f, i) {
                            return page.fontMissing && i === 0 ? f + " (not installed)" : f
                        }))
                        currentIndex: page.savedFont === "" ? 0 : page.fontChoices.indexOf(page.savedFont) + 1
                        onActivated: function (index) {
                            page.set({ font: index === 0 ? "" : page.fontChoices[index - 1] })
                        }
                    }
                }
                LabeledSlider {
                    width: parent.width
                    label: "Text size (px)"
                    from: 6
                    to: 24
                    stepSize: 1
                    integer: true
                    value: page.choice.font_size || 11
                    markValue: 11
                    onSettled: function (v) { page.set({ font_size: Math.round(v) }) }
                }
                FieldRow {
                    width: parent.width
                    label: "Icons"
                    hint: page.bar.scanned
                          ? (page.bar.nerdFont ? "A Nerd Font is installed, so Automatic shows icons."
                                               : "No Nerd Font is installed, so Automatic shows words. "
                                                 + "Install one (any font with \"Nerd Font\" in its name) for icons.")
                          : ""
                    Dropdown {
                        width: 260
                        readonly property var ids: ["auto", "glyphs", "text"]
                        model: ["Automatic", "Nerd Font icons", "Words"]
                        currentIndex: Math.max(0, ids.indexOf(page.choice.icons || "auto"))
                        onActivated: function (index) { page.set({ icons: ids[index] }) }
                    }
                }
            }

            // -----------------------------------------------------------
            Card {
                Layout.fillWidth: true
                title: "What it shows"
                subtitle: "Left to right. A module with nothing to show, such as media while nothing "
                          + "plays or Bluetooth on a machine without it, hides itself."

                Flow {
                    width: parent.width
                    spacing: Theme.gap

                    Repeater {
                        model: page.bar.modules || []
                        Toggle {
                            required property var modelData
                            text: modelData.label
                            checked: page.hasModule(modelData.id)
                            onToggled: page.setModule(modelData.id, checked)
                        }
                    }
                }

                FieldRow {
                    width: parent.width
                    visible: page.hasModule("clock")
                    label: "Clock format"
                    hint: "Qt's date format: ddd is the day's name, MM/dd the month and day, h:mm AP "
                          + "the 12-hour time, HH:mm the 24-hour time."
                    LineEdit {
                        width: parent.width
                        mono: true
                        text: page.choice.clock_format || ""
                        onCommit: function (v) { page.set({ clock_format: v }) }
                    }
                }
                FieldRow {
                    width: parent.width
                    visible: page.hasModule("workspaces")
                    label: "Workspace labels"
                    hint: "For workspaces 1, 2, 3… in order, separated by spaces. Leave empty for "
                          + "their numbers."
                    LineEdit {
                        width: parent.width
                        text: (page.choice.workspace_labels || []).join(" ")
                        placeholderText: "1 2 3 4 5"
                        onCommit: function (v) {
                            page.set({ workspace_labels: v.split(/\s+/).filter(function (s) { return s !== "" }) })
                        }
                    }
                }
                FieldRow {
                    width: parent.width
                    visible: page.hasModule("workspaces")
                    label: "Each monitor's own"
                    hint: "Each bar lists only the workspaces on its monitor."
                    Toggle {
                        checked: page.choice.workspaces_per_monitor === true
                        onToggled: page.set({ workspaces_per_monitor: checked })
                    }
                }
                FieldRow {
                    width: parent.width
                    visible: page.hasModule("media")
                    label: "Media player"
                    hint: "Part of a player's name, such as spotify, to show that one. Empty shows "
                          + "whichever is playing."
                    LineEdit {
                        width: parent.width
                        text: page.choice.player || ""
                        placeholderText: "Whichever is playing"
                        onCommit: function (v) { page.set({ player: v }) }
                    }
                }
                FieldRow {
                    width: parent.width
                    visible: page.hasModule("volume")
                    label: "Right-click on volume"
                    hint: page.bar.mixerFound ? "Empty runs " + page.bar.mixerFound + ", which is installed."
                                              : "No mixer was found (pwvucontrol, pavucontrol…); name a command to run."
                    LineEdit {
                        width: parent.width
                        mono: true
                        text: page.choice.mixer || ""
                        placeholderText: page.bar.mixerFound || "A command"
                        onCommit: function (v) { page.set({ mixer: v }) }
                    }
                }
                FieldRow {
                    width: parent.width
                    visible: page.hasModule("bluetooth")
                    label: "Click on Bluetooth"
                    hint: page.bar.bluetoothFound ? "Empty runs " + page.bar.bluetoothFound + "."
                                                  : "No Bluetooth app was found (blueman, overskride, bluetui…); name a command to run."
                    LineEdit {
                        width: parent.width
                        mono: true
                        text: page.choice.bluetooth || ""
                        placeholderText: page.bar.bluetoothFound || "A command"
                        onCommit: function (v) { page.set({ bluetooth: v }) }
                    }
                }
            }
        }
    }
}
