import QtQuick
import QtQuick.Layouts
import Quickshell.Hyprland

// Hyprland's workspaces: click one to go there. Each shows its name, or the
// label the Bar page gives its number.
RowLayout {
    id: root

    property bool shown: true
    property var screen: null

    visible: shown
    spacing: 0
    Layout.fillHeight: true

    Item { implicitWidth: 14 }

    Repeater {
        model: Hyprland.workspaces.values
            .filter(w => w.id > 0)
            .filter(w => !Config.workspacesPerMonitor || root.screen === null
                         || (w.monitor !== null && w.monitor.name === root.screen.name))
            .sort((a, b) => a.id - b.id)

        delegate: Item {
            id: ws
            required property var modelData
            Layout.fillHeight: true
            implicitWidth: label.implicitWidth + (Config.shaped ? 20 : 12)

            Rectangle {
                anchors.fill: parent
                color: Theme.aqua
                visible: area.containsMouse && !Config.shaped
            }

            // Neumorphic: the focused workspace, and the one under the
            // pointer, are pressed in.
            Neu {
                visible: Config.shaped && (ws.modelData.focused || area.containsMouse)
                anchors { fill: parent; topMargin: 5; bottomMargin: 5; leftMargin: 2; rightMargin: 2 }
                sunk: true
                radius: 4
            }

            Text {
                id: label
                anchors.centerIn: parent
                text: {
                    const l = Config.workspaceLabels[ws.modelData.id - 1];
                    return l !== undefined && l !== "" ? l : ws.modelData.name;
                }
                font.family: Theme.font
                font.pixelSize: Theme.fontSize + 3
                color: area.containsMouse && !Config.shaped ? Theme.hardBackground
                     : ws.modelData.focused ? Theme.red
                     : ws.modelData.active ? Theme.aqua
                     : Theme.foreground
            }

            MouseArea {
                id: area
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: ws.modelData.activate()
            }
        }
    }
}
