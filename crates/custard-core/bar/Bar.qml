import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Wayland

PanelWindow {
    id: bar

    required property var modelData
    screen: modelData

    // Room around the box for its shadow.
    readonly property int pad: 4

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
            model: [0.55, 0.3, 0.14, 0.05]
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
            anchors.fill: parent
            radius: 2
            color: Theme.background
        }

        RowLayout {
            anchors { left: parent.left; top: parent.top; bottom: parent.bottom }
            spacing: 0
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
        }
    }
}
