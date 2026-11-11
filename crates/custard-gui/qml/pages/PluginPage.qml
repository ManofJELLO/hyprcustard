import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import dev.hyprcustard.gui

// Installing, updating and loading the plugin itself, with hyprpm.
//
// Everything here shells out to `hyprpm`, which takes minutes when it rebuilds
// against a new Hyprland and asks for a password partway through. So the page
// is built around the output rather than around the buttons: what it is doing
// is the part worth looking at.
Item {
    id: page

    // What `hyprpm list` last said.
    property var pluginStatus: ({ installed: false, plugins: [] })
    property int logLines: 0
    // True when the last line is a progress bar, which the next one replaces.
    property bool lastTransient: false
    // Auto-scroll, until the reader turns it off to look at something.
    property bool follow: true

    readonly property string pluginName: App.settings.plugin_name || "HyprWindowShade"
    readonly property string repoUrl: App.settings.plugin_repo_url || ""

    readonly property bool hyprpmPresent: page.pluginStatus.installed === true
    readonly property string statusError: page.pluginStatus.error || ""

    // The plugin this app is a front end for, among everything hyprpm has.
    readonly property var entry: {
        var list = page.pluginStatus.plugins || []
        for (var i = 0; i < list.length; ++i)
            if (list[i].name === page.pluginName || list[i].repository === page.pluginName)
                return list[i]
        return null
    }

    readonly property bool installed: page.entry !== null
    readonly property bool pluginEnabled: page.entry !== null && page.entry.enabled === true
    readonly property bool busy: App.hyprpmBusy

    // What "run this in a terminal instead" should run.
    readonly property string mainOp: page.installed ? "update" : "add"
    readonly property string mainArg: page.installed ? "" : page.repoUrl

    Component.onCompleted: page.refreshStatus()

    function refreshStatus() {
        page.pluginStatus = App.hyprpmStatus()
    }

    function updateSettings(changes) {
        var copy = JSON.parse(JSON.stringify(App.settings))
        for (var k in changes)
            copy[k] = changes[k]
        App.run("settings.update", { settings: copy })
    }

    function appendLine(line, isTransient) {
        // hyprpm redraws its progress bar with a carriage return. On a
        // terminal that overwrites the line; here it would be hundreds of
        // near-identical rows, so the new frame takes the old one's place.
        if (page.lastTransient) {
            var cut = log.text.lastIndexOf("\n")
            log.text = cut >= 0 ? log.text.substring(0, cut) : ""
            page.logLines = Math.max(0, page.logLines - 1)
        }
        page.lastTransient = isTransient === true

        log.append(line)
        page.logLines += 1

        // A Hyprland rebuild prints tens of thousands of lines, and none of
        // the early ones matter once it has got that far.
        if (page.logLines > 6000) {
            var kept = log.text.split("\n").slice(-3000)
            log.text = kept.join("\n")
            page.logLines = kept.length
        }
        if (page.follow)
            page.scrollToEnd()
    }

    function scrollToEnd() {
        var bar = logScroll.ScrollBar.vertical
        bar.position = Math.max(0, 1 - bar.size)
    }

    Connections {
        target: App.backend

        function onHyprpmStarted(label) {
            page.appendLine("", false)
            page.appendLine("── " + label + " ──", false)
        }

        function onHyprpmOutput(line, isTransient) {
            page.appendLine(line, isTransient)
        }

        function onHyprpmFinished(ok, message) {
            page.appendLine(message, false)
            page.refreshStatus()
        }
    }

    // The output pane wants all the room there is, but this window is often a
    // tile half the height of the screen — where "all the room there is" is
    // none, and an output pane nobody can reach is worse than a short one. So
    // the page scrolls like the others, and the pane takes what is left over
    // only when there is something left over.
    ScrollView {
        id: scroll
        anchors.fill: parent
        clip: true
        contentWidth: availableWidth

        ColumnLayout {
            width: Math.min(scroll.availableWidth, Theme.contentMax)
            x: Math.round((scroll.availableWidth - width) / 2)
            spacing: Theme.pad

        // -------------------------------------------------------------------
        // What is installed, and what can be done about it
        // -------------------------------------------------------------------
        Card {
            id: statusCard
            Layout.fillWidth: true
            title: "Plugin"
            subtitle: "hyprpm builds the plugin against the Hyprland you are running, so it "
                      + "has to be updated whenever Hyprland is. It asks for your password "
                      + "for the parts that write outside your home directory."

            RowLayout {
                width: parent.width
                spacing: Theme.gapSmall

                Badge {
                    text: page.hyprpmPresent ? "hyprpm found" : "hyprpm not installed"
                    tint: page.hyprpmPresent ? Theme.ok : Theme.error
                }
                Badge {
                    visible: page.hyprpmPresent
                    text: page.installed ? "installed" : "not installed"
                    tint: page.installed ? Theme.ok : Theme.muted
                }
                Badge {
                    visible: page.installed
                    text: page.pluginEnabled ? "enabled" : "disabled"
                    tint: page.pluginEnabled ? Theme.ok : Theme.warn
                }
                Badge {
                    visible: App.hyprlandRunning
                    text: App.pluginLoaded ? "loaded" : "not loaded"
                    tint: App.pluginLoaded ? Theme.ok : Theme.warn
                }

                Item {
                    Layout.fillWidth: true
                }

                PillButton {
                    text: "Check again"
                    tooltip: "Run hyprpm list and read the answer"
                    onClicked: page.refreshStatus()
                }
            }

            Text {
                width: parent.width
                visible: !page.hyprpmPresent
                text: "hyprpm comes with Hyprland — on Arch it is the hyprpm package. Without "
                      + "it, the plugin has to be built and loaded by hand."
                color: Theme.warn
                font.pixelSize: Theme.fontSize
                wrapMode: Text.WordWrap
            }

            Text {
                width: parent.width
                visible: page.statusError !== ""
                text: page.statusError
                color: Theme.error
                font.pixelSize: Theme.fontSize
                wrapMode: Text.WordWrap
            }

            Text {
                width: parent.width
                visible: page.entry !== null && page.entry.author !== ""
                text: page.pluginName + " · from " + (page.entry ? page.entry.repository : "")
                      + " by " + (page.entry ? page.entry.author : "")
                color: Theme.muted
                font.pixelSize: Theme.fontSizeSmall
            }

            Flow {
                width: parent.width
                spacing: Theme.gapSmall

                PillButton {
                    text: "Install"
                    primary: true
                    visible: !page.installed
                    enabled: page.hyprpmPresent && !page.busy && page.repoUrl !== ""
                    tooltip: "hyprpm add " + page.repoUrl
                    onClicked: App.hyprpm("add", page.repoUrl)
                }

                PillButton {
                    text: "Update"
                    primary: page.installed
                    enabled: page.hyprpmPresent && !page.busy
                    tooltip: "hyprpm update — rebuilds every plugin against the Hyprland you "
                             + "are running now. This is the slow one."
                    onClicked: App.hyprpm("update", "")
                }

                PillButton {
                    text: page.pluginEnabled ? "Disable" : "Enable"
                    visible: page.installed
                    enabled: page.hyprpmPresent && !page.busy
                    tooltip: (page.pluginEnabled ? "hyprpm disable " : "hyprpm enable ") + page.pluginName
                    onClicked: App.hyprpm(page.pluginEnabled ? "disable" : "enable", page.pluginName)
                }

                PillButton {
                    text: "Reload"
                    enabled: page.hyprpmPresent && !page.busy
                    tooltip: "hyprpm reload — load enabled plugins into the running compositor"
                    onClicked: App.hyprpm("reload", "")
                }

                PillButton {
                    text: "Run in a terminal"
                    enabled: page.hyprpmPresent && !page.busy
                           && (page.installed || page.repoUrl !== "")
                    tooltip: "Runs " + (page.installed ? "hyprpm update" : "hyprpm add")
                             + " in a terminal window instead, where sudo asks for the password "
                             + "the way it always has"
                    onClicked: App.hyprpmInTerminal(page.mainOp, page.mainArg)
                }

                PillButton {
                    text: "Remove"
                    danger: true
                    visible: page.installed
                    enabled: page.hyprpmPresent && !page.busy
                    onClicked: App.confirm(
                        "Remove " + page.pluginName + "?",
                        "hyprpm will uninstall the plugin. Your rules, shaders and config are "
                        + "not touched, and it can be installed again from here.",
                        "Remove", true,
                        function () {
                            App.hyprpm("remove", page.pluginName)
                        })
                }
            }

            FieldRow {
                width: parent.width
                label: "Repository"
                hint: "Where Install takes the plugin from. Change it to install a fork."

                LineEdit {
                    width: parent.width
                    mono: true
                    text: page.repoUrl
                    onCommit: function (v) {
                        page.updateSettings({ plugin_repo_url: v.trim() })
                    }
                }
            }

            FieldRow {
                width: parent.width
                label: "Plugin name"
                hint: "What hyprpm calls it — the name enable, disable and remove take."

                LineEdit {
                    width: 260
                    mono: true
                    text: page.pluginName
                    onCommit: function (v) {
                        page.updateSettings({ plugin_name: v.trim() })
                    }
                }
            }
        }

        // -------------------------------------------------------------------
        // Output
        // -------------------------------------------------------------------
        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: Math.max(
                220, scroll.availableHeight - statusCard.implicitHeight - Theme.pad)
            color: Theme.surface
            radius: Theme.radius
            border.width: 1
            border.color: Theme.border

            ColumnLayout {
                anchors.fill: parent
                anchors.margins: Theme.pad
                spacing: Theme.gap

                RowLayout {
                    Layout.fillWidth: true
                    spacing: Theme.gapSmall

                    Text {
                        text: page.busy ? App.hyprpmLabel : "Output"
                        color: Theme.fg
                        font.pixelSize: Theme.fontSizeLarge
                        font.weight: Font.DemiBold
                    }

                    // A build says nothing for minutes at a time; this is how
                    // the window shows it has not died.
                    Badge {
                        visible: page.busy
                        text: "running"
                        tint: Theme.accent

                        SequentialAnimation on opacity {
                            running: page.busy
                            loops: Animation.Infinite
                            NumberAnimation { to: 0.35; duration: 700 }
                            NumberAnimation { to: 1.0; duration: 700 }
                        }
                    }

                    Item {
                        Layout.fillWidth: true
                    }

                    Text {
                        text: "Follow"
                        color: Theme.muted
                        font.pixelSize: Theme.fontSizeSmall
                    }

                    Toggle {
                        checked: page.follow
                        onToggled: {
                            page.follow = checked
                            if (page.follow)
                                page.scrollToEnd()
                        }
                    }

                    PillButton {
                        text: "Clear"
                        enabled: page.logLines > 0
                        onClicked: {
                            log.text = ""
                            page.logLines = 0
                            page.lastTransient = false
                        }
                    }

                    PillButton {
                        text: "Cancel"
                        danger: true
                        enabled: page.busy
                        tooltip: "Stop hyprpm and whatever it is building"
                        onClicked: App.hyprpmCancel()
                    }
                }

                Rectangle {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    color: Theme.bgAlt
                    radius: Theme.radiusSmall
                    border.width: 1
                    border.color: Theme.border

                    ScrollView {
                        id: logScroll
                        anchors.fill: parent
                        anchors.margins: 1
                        clip: true

                        // With nothing in it to scroll, the pane should not
                        // swallow the wheel: on a short window the page
                        // behind it is the thing that needs to move.
                        Component.onCompleted: contentItem.interactive = Qt.binding(
                            function () {
                                return contentItem.contentHeight > contentItem.height
                            })

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

                    EmptyState {
                        anchors.centerIn: parent
                        width: parent.width - 2 * Theme.pad
                        visible: page.logLines === 0
                        title: "Nothing has run yet"
                        body: "Install, Update and Reload print what they are doing here."
                    }
                }
            }
        }

        }
    }
}
