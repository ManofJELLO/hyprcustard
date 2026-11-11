import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import dev.hyprcustard.gui

// The shader, running.
//
// Not a simulation of the plugin — a second Hyprland with the plugin loaded
// into it, rendering a temporary copy of the shader with whatever values are
// staged right now. The window in it opens, holds and closes on a loop, which
// is the only way to see an open or close shader at all.
//
// Frames arrive as a file on disk that the backend rewrites; the cache buster
// on the URL is what makes Qt look at it again.
Item {
    id: pane

    property string shaderPath: ""
    property string shaderName: ""

    // What the backend was asked for. The image is scaled to fit afterwards,
    // so resizing the pane costs sharpness rather than a restart.
    readonly property int captureWidth: 760
    readonly property int captureHeight: 480

    readonly property bool showingThis: App.previewRunning
                                        && App.previewShader === pane.shaderPath

    // True from the first frame drawn until the preview is not this shader's
    // any more. What is on screen cannot be keyed on the image's own status:
    // the frame is replaced ten times a second, and every gap between one
    // frame and the next — or a single read that comes back late or empty —
    // would drop the pane back to its "Not running" placeholder and flash it
    // over a preview that is running perfectly well.
    property bool hasFrame: false

    onShowingThisChanged: {
        if (!pane.showingThis)
            pane.hasFrame = false
    }

    // Stand the window on the user's own wallpaper, or on the pane's colour.
    property bool desktopBackdrop: true

    implicitWidth: 380
    implicitHeight: 300

    function start() {
        // The pane's own colour, so the frame's empty space and the pane it is
        // drawn on are the same shade and the seam disappears.
        App.previewStart(pane.shaderPath, pane.captureWidth, pane.captureHeight,
                         Theme.bgAlt, pane.desktopBackdrop)
    }

    // Anything that means "you are not looking at this any more" has to take
    // the compositor with it: nothing else will.
    //
    // The guard is on more than `previewRunning`, because that is false for
    // the several seconds a start is in flight — and a start abandoned
    // half-way still lands, leaving a compositor cycling terminals for a pane
    // nobody can see. Stopping when nothing is running is a no-op anyway.
    function stopIfMine() {
        pane.hasFrame = false
        if (App.previewRunning || App.previewBusy)
            App.previewStop()
    }

    // Pages live in a StackLayout and are all built at startup, so this fires
    // at window teardown and not a moment before. Leaving the page is
    // `onVisibleChanged` below; this is the app closing.
    Component.onDestruction: pane.stopIfMine()

    // Which is why visibility matters: a hidden page is still a live object
    // with a live timer, and without this the preview would go on running —
    // and grabbing ten frames a second — behind whatever page you switched to.
    onVisibleChanged: {
        if (!pane.visible) {
            pane.stopIfMine()
            frame.source = ""
        }
    }

    onShaderPathChanged: {
        pane.stopIfMine()
        frame.source = ""
    }

    // Any command may have changed what is staged, and pushing it is one file
    // write that the plugin notices by itself.
    Connections {
        target: App
        function onStateChanged() {
            if (pane.showingThis)
                App.previewUpdate(pane.shaderPath)
        }
    }

    Timer {
        // Ten frames a second: enough to read an animation, few enough that a
        // grim per frame stays out of the way.
        interval: 100
        running: pane.showingThis && pane.visible
        repeat: true
        onTriggered: {
            var f = App.previewFrame()
            if (f.path)
                frame.source = "file://" + f.path + "?" + f.n
        }
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: Theme.gap

        RowLayout {
            Layout.fillWidth: true
            spacing: Theme.gapSmall

            Text {
                text: "Preview"
                color: Theme.fg
                font.pixelSize: Theme.fontSize
                font.weight: Font.DemiBold
            }

            Badge {
                visible: pane.showingThis
                text: "live"
                tint: Theme.ok
            }

            Item {
                Layout.fillWidth: true
            }

            PillButton {
                compact: true
                text: pane.desktopBackdrop ? "On your wallpaper" : "On a plain colour"
                tooltip: pane.desktopBackdrop
                         ? "Standing on your own wallpaper, photographed without any of your "
                           + "windows in the way. Click for a plain colour instead."
                         : "Standing on the pane's own colour. Click to stand it on your "
                           + "wallpaper instead."
                onClicked: {
                    pane.desktopBackdrop = !pane.desktopBackdrop
                    // Straight to start, with no stop first: starting already
                    // tears down whatever is running, and doing it here too
                    // races — the stop runs on a thread of its own, so a slow
                    // one could land after the new preview and kill it.
                    if (pane.showingThis)
                        pane.start()
                }
            }

            PillButton {
                compact: true
                enabled: !App.previewBusy && App.hyprlandRunning && pane.shaderPath !== ""
                text: App.previewBusy ? "Starting…" : (pane.showingThis ? "Stop" : "Run it")
                tooltip: App.hyprlandRunning
                         ? "Run this shader in a Hyprland of its own and watch it here"
                         : "Hyprland is not running, so there is nothing to run a preview in"
                onClicked: pane.showingThis ? App.previewStop() : pane.start()
            }
        }

        Rectangle {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.minimumHeight: 180
            color: Theme.bgAlt
            radius: Theme.radiusSmall
            border.width: 1
            border.color: Theme.border
            clip: true

            Image {
                id: frame
                anchors.fill: parent
                anchors.margins: 1
                fillMode: Image.PreserveAspectFit
                // The file keeps its name and changes underneath us, so Qt has
                // to be told not to trust what it already read.
                cache: false
                // Loaded on the spot rather than on the loader thread. An
                // asynchronous load leaves the image without a picture for the
                // moment it takes, and at ten frames a second that moment is
                // most of them. The tick this runs on has already waited for a
                // `grim` of its own, so a small PNG read costs it nothing it
                // was not already spending.
                asynchronous: false
                visible: pane.showingThis && pane.hasFrame
                smooth: true
                onStatusChanged: {
                    if (status === Image.Ready)
                        pane.hasFrame = true
                }
            }

            EmptyState {
                anchors.centerIn: parent
                width: Math.min(300, parent.width - Theme.pad * 2)
                visible: !frame.visible
                title: App.previewBusy
                       ? "Starting a compositor…"
                       : (App.hyprlandRunning ? "Not running" : "Hyprland is not running")
                body: {
                    if (App.previewBusy)
                        return "A second Hyprland is coming up with the plugin loaded."
                    if (!App.hyprlandRunning)
                        return "The preview runs a nested Hyprland, so it needs a session to "
                             + "nest inside."
                    return "Run it to see " + (pane.shaderName === "" ? "this shader"
                                                                      : pane.shaderName)
                         + " on a real window, with the values you have staged."
                }
            }
        }

        Text {
            Layout.fillWidth: true
            visible: pane.showingThis
            text: "The window opens, holds and closes on a loop — an open or close shader "
                  + "only exists during that. Values you change here are written to a copy, "
                  + "never to your shader."
            color: Theme.muted
            font.pixelSize: Theme.fontSizeSmall
            wrapMode: Text.WordWrap
        }
    }
}
