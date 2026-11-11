import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import dev.hyprcustard.gui

ApplicationWindow {
    id: window

    width: 1180
    height: 780
    // Low enough to sit in a tile on a short screen. Every page scrolls, so
    // a small window loses nothing but the amount visible at once.
    minimumWidth: 720
    minimumHeight: 400
    visible: true
    title: "HyprCustard"
           + (App.dirty || App.dirtyShaders > 0 ? " — unsaved changes" : "")
    color: Theme.bg
    opacity: Theme.windowOpacity

    // Everything in this app styles its own background and contentItem, with
    // two exceptions, both of them things Qt builds for itself where there is
    // no handle to style: the scrollbar inside each ScrollView, and — since Qt
    // 6.9 — the cut/copy/paste menu on every text field and text area. Both
    // draw from the palette, so the roles they read are set once here rather
    // than by giving eight ScrollViews a ScrollBar apiece and every text
    // control a menu of its own.
    //
    // The menu roles are `window` and `windowText` for the popup itself,
    // `light` and `midlight` for an item hovered and pressed, `dark` for its
    // border, `mid` for the separators and `shadow` for the drop shadow.
    palette.mid: Theme.border
    palette.dark: Theme.border
    palette.text: Theme.fg
    palette.window: Theme.bgAlt
    palette.windowText: Theme.fg
    palette.light: Theme.surfaceHi
    palette.midlight: Theme.selection
    palette.shadow: "#000000"
    palette.highlight: Theme.accent
    palette.highlightedText: Theme.onAccent

    readonly property var pages: [
        { label: "Look",     hint: "Borders, blur, gaps" },
        { label: "Rules",    hint: "Windows" },
        { label: "Shaders",  hint: "Files" },
        { label: "Layers",   hint: "Bars, launchers" },
        { label: "Keybinds", hint: "And startup" },
        { label: "Preview",  hint: "And import" },
        { label: "Plugin",   hint: "Install, update" },
        { label: "Settings", hint: "" }
    ]

    property int currentPage: 0

    // The two QML singletons are fed from here rather than reading Backend
    // themselves, which keeps the module free of circular dependencies.
    Component.onCompleted: {
        App.backend = Backend
        App.confirmDialog = confirm
        window.syncState()
    }

    function syncState() {
        App.rawState = Backend.stateJson
        Theme.rawState = Backend.stateJson
    }

    Connections {
        target: Backend

        function onStateJsonChanged() {
            window.syncState()
        }

        function onNotify(message, isError) {
            toast.show(message, isError)
        }

        // hyprpm escalates by itself and, with no terminal to read from, sudo
        // asks this app for the password. See components/PasswordDialog.qml.
        function onHyprpmPasswordRequested(prompt, retry) {
            password.ask(prompt, retry)
        }
    }

    // The compositor's window list changes while the app is open, so the
    // pickers are refreshed periodically rather than only at startup.
    //
    // The probe runs on a background thread, so this costs the interface
    // nothing; it is still paused while the window is in the background,
    // because three hyprctl processes every five seconds for a list nobody is
    // looking at is rude. Firing on start means switching back refreshes at
    // once rather than showing a stale list until the next tick.
    Timer {
        interval: 5000
        running: window.active
        repeat: true
        triggeredOnStart: true
        onTriggered: App.refreshLive()
    }

    // Nothing here is written to disk until Save, so closing with unsaved work
    // loses it silently. The title bar says so; that is not enough.
    //
    // A running hyprpm operation is the other thing worth stopping for: it
    // carries on in a session of its own, but with this window gone there is
    // nobody left to answer sudo if it asks again.
    onClosing: function (close) {
        // The preview is a whole compositor; it does not get to stay behind.
        // Its own watchdog would reap it within a couple of seconds anyway,
        // but not before the user has watched the window vanish and wondered.
        App.previewStop()

        if (App.hyprpmBusy && !window.hyprpmCloseConfirmed) {
            close.accepted = false
            confirm.ask("Close while hyprpm is working?",
                        App.hyprpmLabel + " is still running. It will carry on without this "
                        + "window, but if it needs your password again it will fail.",
                        "Close anyway", true,
                        function () {
                            window.hyprpmCloseConfirmed = true
                            window.close()
                        })
            return
        }
        if ((App.dirty || App.dirtyShaders > 0) && !window.quitConfirmed) {
            close.accepted = false
            confirm.ask("Close without saving?",
                        window.unsavedSummary(),
                        "Discard and close", true,
                        function () {
                            window.quitConfirmed = true
                            window.close()
                        })
        }
    }

    property bool quitConfirmed: false
    property bool hyprpmCloseConfirmed: false

    // Two kinds of unsaved work, written by two different buttons to two
    // different places, so the warning has to name whichever ones apply.
    function unsavedSummary() {
        var parts = []
        if (App.dirty)
            parts.push("Your rules, layers and keybinds have not been written to "
                       + App.configPathDisplay + ".")
        if (App.dirtyShaders > 0)
            parts.push(App.plural(App.dirtyShaders, "shader")
                       + (App.dirtyShaders === 1 ? " has" : " have")
                       + " changes that were never written to the file.")
        return parts.join(" ")
    }

    Shortcut {
        sequences: [StandardKey.Save]
        onActivated: App.run("config.save", {})
    }
    Shortcut {
        sequences: [StandardKey.Refresh]
        onActivated: App.refresh()
    }

    // Everything lives inside one item so the whole interface can be captured
    // as a single image, which is what the screenshot aid below grabs.
    ColumnLayout {
        id: shell
        anchors.fill: parent
        spacing: 0

    // -----------------------------------------------------------------------
    // Header
    // -----------------------------------------------------------------------
    Rectangle {
        Layout.fillWidth: true
        Layout.preferredHeight: 58
        implicitHeight: 58
        color: Theme.bgAlt

        Rectangle {
            anchors.bottom: parent.bottom
            width: parent.width
            height: 1
            color: Theme.border
        }

        RowLayout {
            anchors.fill: parent
            anchors.leftMargin: Theme.pad
            anchors.rightMargin: Theme.pad
            spacing: Theme.gap

            ColumnLayout {
                spacing: 0

                Text {
                    text: "HyprCustard"
                    color: Theme.fg
                    font.pixelSize: Theme.fontSizeTitle
                    font.weight: Font.DemiBold
                }
                Text {
                    text: App.configPathDisplay
                    color: Theme.muted
                    font.pixelSize: Theme.fontSizeSmall
                    font.family: Theme.monoFamily
                }
            }

            Item {
                Layout.fillWidth: true
            }

            // Live status.
            Row {
                spacing: Theme.gapSmall

                Badge {
                    text: App.hyprlandRunning ? "Hyprland up" : "Hyprland not running"
                    tint: App.hyprlandRunning ? Theme.ok : Theme.muted
                }
                Badge {
                    visible: App.hyprlandRunning
                    text: App.pluginLoaded ? "plugin loaded" : "plugin not loaded"
                    tint: App.pluginLoaded ? Theme.ok : Theme.warn
                }
                Badge {
                    visible: App.dirty
                    text: "unsaved"
                    tint: Theme.warn
                }
                Badge {
                    visible: App.dirtyShaders > 0
                    text: App.plural(App.dirtyShaders, "shader") + " edited"
                    tint: Theme.warn
                }
            }

            PillButton {
                text: "Refresh"
                tooltip: "Re-read the shader folder and ask Hyprland what is on screen"
                onClicked: App.refresh()
            }

            PillButton {
                text: "Save"
                primary: true
                enabled: App.dirty
                tooltip: "Write the managed block into your Hyprland config"
                onClicked: App.run("config.save", {})
            }
        }
    }

    // -----------------------------------------------------------------------
    // Body
    // -----------------------------------------------------------------------
    RowLayout {
        Layout.fillWidth: true
        Layout.fillHeight: true
        spacing: 0

        // --- navigation ---
        Rectangle {
            Layout.preferredWidth: Theme.sidebarWidth
            Layout.fillHeight: true
            color: Theme.bgAlt

            Rectangle {
                anchors.right: parent.right
                width: 1
                height: parent.height
                color: Theme.border
            }

            ColumnLayout {
                anchors.fill: parent
                anchors.margins: Theme.gap
                spacing: 2

                Repeater {
                    model: window.pages

                    Rectangle {
                        id: navItem

                        required property var modelData
                        required property int index

                        Layout.fillWidth: true
                        Layout.preferredHeight: 44
                        radius: Theme.radiusSmall
                        color: window.currentPage === navItem.index
                               ? Theme.selection
                               : (navHover.hovered ? Theme.surfaceHi : "transparent")

                        HoverHandler {
                            id: navHover
                        }
                        MouseArea {
                            anchors.fill: parent
                            onClicked: window.currentPage = navItem.index
                        }

                        Rectangle {
                            width: 3
                            height: 22
                            radius: 2
                            anchors.left: parent.left
                            anchors.verticalCenter: parent.verticalCenter
                            color: Theme.accent
                            visible: window.currentPage === navItem.index
                        }

                        Column {
                            anchors.verticalCenter: parent.verticalCenter
                            anchors.left: parent.left
                            anchors.leftMargin: Theme.pad
                            anchors.right: parent.right
                            anchors.rightMargin: Theme.gapSmall
                            spacing: 0

                            Text {
                                text: navItem.modelData.label
                                color: window.currentPage === navItem.index ? Theme.fg : Theme.fgDim
                                font.pixelSize: Theme.fontSize
                                font.weight: window.currentPage === navItem.index
                                             ? Font.DemiBold : Font.Normal
                            }
                            Text {
                                text: navItem.modelData.hint
                                visible: text !== ""
                                color: Theme.muted
                                font.pixelSize: Theme.fontSizeSmall
                                elide: Text.ElideRight
                                width: parent.width
                            }
                        }
                    }
                }

                Item {
                    Layout.fillHeight: true
                }

                Text {
                    Layout.fillWidth: true
                    text: App.shaders.length + " shader" + (App.shaders.length === 1 ? "" : "s")
                          + "  ·  " + App.rules.length + " rule"
                          + (App.rules.length === 1 ? "" : "s")
                    color: Theme.muted
                    font.pixelSize: Theme.fontSizeSmall
                    wrapMode: Text.WordWrap
                }

                Text {
                    Layout.fillWidth: true
                    text: Theme.name
                    color: Theme.muted
                    font.pixelSize: Theme.fontSizeSmall
                    elide: Text.ElideRight
                }
            }
        }

        // --- pages ---
        StackLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            currentIndex: window.currentPage

            Item {
                LookPage {
                    anchors.fill: parent
                    anchors.margins: Theme.pad
                }
            }
            Item {
                RulesPage {
                    anchors.fill: parent
                    anchors.margins: Theme.pad
                }
            }
            Item {
                ShadersPage {
                    anchors.fill: parent
                    anchors.margins: Theme.pad
                }
            }
            Item {
                LayersPage {
                    anchors.fill: parent
                    anchors.margins: Theme.pad
                }
            }
            Item {
                BindsPage {
                    anchors.fill: parent
                    anchors.margins: Theme.pad
                }
            }
            Item {
                PreviewPage {
                    anchors.fill: parent
                    anchors.margins: Theme.pad
                }
            }
            Item {
                PluginPage {
                    anchors.fill: parent
                    anchors.margins: Theme.pad
                }
            }
            Item {
                SettingsPage {
                    anchors.fill: parent
                    anchors.margins: Theme.pad
                }
            }
        }
    }

    }

    Toast {
        id: toast
        anchors.fill: parent
    }

    // Above the toast: a question has to be answerable even while one is up.
    ConfirmDialog {
        id: confirm
    }

    // Above everything: sudo is waiting for this one.
    PasswordDialog {
        id: password
        onAnswered: function (value) {
            Backend.hyprpmAnswerPassword(value)
        }
        onRefused: Backend.hyprpmCancelPassword()
    }

    // Development aid: with CUSTARD_SHOT_DIR set, render each page to a PNG there
    // and quit. Lets the layout be checked without a compositor.
    Loader {
        active: Backend.shotDir !== ""
        sourceComponent: Timer {
            interval: 700
            running: true
            repeat: true
            onTriggered: {
                shell.grabToImage(function (result) {
                    result.saveToFile(Backend.shotDir + "/" + window.currentPage + "-"
                                      + window.pages[window.currentPage].label.toLowerCase()
                                      + ".png")
                    if (window.currentPage + 1 < window.pages.length)
                        window.currentPage = window.currentPage + 1
                    else
                        Qt.quit()
                })
            }
        }
    }
}
