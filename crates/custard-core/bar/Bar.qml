import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Wayland

PanelWindow {
    id: bar

    required property var modelData
    required property var launcher
    required property var powerMenu
    screen: modelData

    // The buttons call these; say so at once if either is missing, rather
    // than only when a button is clicked.
    Component.onCompleted: {
        if (!bar.launcher || !bar.powerMenu)
            console.error("Bar.qml: the bar was given no launcher or power menu");
    }

    // Room around the box for its shadow.
    // Room around the box for its shadow: a neumorphic shadow reaches
    // further than the flat one.
    readonly property int pad: Config.neumorphic ? Theme.reach(Theme.depth) + 1 : 4

    anchors { top: !Config.bottom; bottom: Config.bottom; left: true; right: true }
    margins {
        top: Config.bottom ? 0 : Config.edgeMargin
        bottom: Config.bottom ? Config.edgeMargin : 0
        left: Config.sideMargin
        right: Config.sideMargin
    }
    // The box, the room for its shadow, and the space kept clear on the
    // windows' side.
    implicitHeight: Config.height + 2 * pad + Config.windowGap
    color: "transparent"
    WlrLayershell.layer: WlrLayer.Top
    WlrLayershell.namespace: "quickshell"

    Item {
        id: box
        anchors {
            fill: parent
            leftMargin: bar.pad
            rightMargin: bar.pad
            topMargin: bar.pad + (Config.bottom ? Config.windowGap : 0)
            bottomMargin: bar.pad + (Config.bottom ? 0 : Config.windowGap)
        }

        // A soft shadow drawn outside the box only, so the translucent
        // background is not darkened from underneath.
        Repeater {
            model: Config.neumorphic ? [] : [0.55, 0.3, 0.14, 0.05]
            delegate: Rectangle {
                required property real modelData
                required property int index
                anchors.fill: parent
                anchors.margins: -(index + 1)
                radius: 2 + index + 1
                color: "transparent"
                border.width: 1
                border.color: Qt.rgba(Theme.shadow.r, Theme.shadow.g, Theme.shadow.b, modelData)
            }
        }

        Rectangle {
            visible: !Config.neumorphic
            anchors.fill: parent
            radius: 2
            color: Theme.background
        }

        Neu {
            visible: Config.neumorphic
            anchors.fill: parent
            radius: 8
        }

        RowLayout {
            anchors { left: parent.left; top: parent.top; bottom: parent.bottom }
            spacing: 0
            LauncherModule { shown: Config.has("launcher"); launcher: bar.launcher }
            ClockModule { shown: Config.has("clock") }
            WorkspacesModule { shown: Config.has("workspaces"); screen: bar.screen }
        }

        WindowModule {
            anchors.centerIn: parent
            height: parent.height
            shown: Config.has("window")
        }

        // Anchored on the right, so a module changing width moves only the
        // ones to its left. The two that come and go (what is playing, and
        // tray icons) are therefore at the far left; the rest keep their
        // place and reserve the width of their widest text.
        RowLayout {
            anchors { right: parent.right; top: parent.top; bottom: parent.bottom }
            spacing: 0
            MediaModule { shown: Config.has("media") }
            TrayModule { bar: bar; shown: Config.has("tray") }
            VolumeModule { shown: Config.has("volume") }
            BluetoothModule { shown: Config.has("bluetooth") }
            NetworkModule { shown: Config.has("network") }
            CpuModule { shown: Config.has("cpu") }
            MemoryModule { shown: Config.has("memory") }
            TempModule { kind: "cpu"; glyph: 0xf050f; shown: Config.has("cpu_temp") }
            TempModule { kind: "gpu"; glyph: 0xf08ae; shown: Config.has("gpu_temp") }
            PowerModule { shown: Config.has("power"); menu: bar.powerMenu }
        }
    }
}
