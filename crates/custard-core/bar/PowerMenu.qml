import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Hyprland
import Quickshell.Io
import Quickshell.Wayland

// The power menu: lock, log out, suspend, reboot, shut down. The bar's power
// button opens it, and so does `qs ipc -p <this folder> call power toggle`.
// Log out, reboot and shut down ask first, in the row under the buttons,
// which is always there so the buttons never move: Enter or Y says yes, Esc
// or N goes back. Left and Right (also Tab) move between buttons.
PanelWindow {
    id: root

    property bool open: false
    property int selected: 0
    // The action waiting for a yes, and which answer is picked (0 yes, 1 no).
    property var confirming: null
    property int answer: 0
    property double clearedAt: 0
    property real uptime: 0

    readonly property var actions: [
        { id: "lock", label: "Lock", glyph: 0xf033e, ask: false },
        { id: "logout", label: "Log out", glyph: 0xf0343, ask: true },
        { id: "suspend", label: "Suspend", glyph: 0xf04b2, ask: false },
        { id: "reboot", label: "Reboot", glyph: 0xf0709, ask: true },
        { id: "shutdown", label: "Shut down", glyph: 0xf0425, ask: true }
    ].filter(a => (Config.power[a.id] ?? "") !== "")

    readonly property int button: Math.round(Theme.fontSize * 8)
    readonly property int gap: 8

    function toggle() {
        if (open)
            hide();
        else
            show();
    }

    function show() {
        if (open || Date.now() - clearedAt < 300)
            return;
        const name = Hyprland.focusedMonitor ? Hyprland.focusedMonitor.name : "";
        root.screen = Quickshell.screens.find(s => s.name === name) ?? Quickshell.screens[0] ?? null;
        root.selected = 0;
        root.confirming = null;
        uptimeFile.reload();
        open = true;
        keys.forceActiveFocus();
    }

    function hide() {
        open = false;
        confirming = null;
    }

    function choose(a) {
        if (!a)
            return;
        if (a.ask) {
            root.confirming = a;
            root.answer = 0;
        } else {
            run(a);
        }
    }

    function run(a) {
        const command = Config.power[a.id] ?? "";
        hide();
        if (command !== "")
            Quickshell.execDetached(["sh", "-c", command]);
    }

    function uptimeText() {
        const m = Math.floor(root.uptime / 60);
        const d = Math.floor(m / 1440), h = Math.floor(m % 1440 / 60), min = m % 60;
        const part = (n, w) => n + " " + w + (n === 1 ? "" : "s");
        const parts = [];
        if (d > 0)
            parts.push(part(d, "day"));
        if (h > 0)
            parts.push(part(h, "hour"));
        if (d === 0)
            parts.push(part(min, "minute"));
        return "Up " + parts.join(", ");
    }

    FileView {
        id: uptimeFile
        path: "/proc/uptime"
        printErrors: false
        onLoaded: root.uptime = Number(text().split(" ")[0]) || 0
    }

    visible: open
    color: "transparent"
    implicitWidth: panel.pad * 2 + root.gap * 2 + root.actions.length * root.button
                   + Math.max(0, root.actions.length - 1) * root.gap
    implicitHeight: panel.pad * 2 + root.gap * 3 + root.button + answerRow.height
    exclusionMode: ExclusionMode.Ignore
    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.namespace: "quickshell"
    WlrLayershell.keyboardFocus: open ? WlrKeyboardFocus.Exclusive : WlrKeyboardFocus.None

    onVisibleChanged: if (visible) keys.forceActiveFocus()

    HyprlandFocusGrab {
        windows: [root]
        active: root.visible
        onCleared: {
            root.clearedAt = Date.now();
            root.hide();
        }
    }

    Item {
        id: keys
        focus: true
        Keys.onPressed: event => {
            const k = event.key;
            const n = root.actions.length;
            if (root.confirming) {
                if (k === Qt.Key_Left || k === Qt.Key_Right || k === Qt.Key_Tab || k === Qt.Key_Backtab)
                    root.answer = 1 - root.answer;
                else if (k === Qt.Key_Y)
                    root.run(root.confirming);
                else if (k === Qt.Key_Return || k === Qt.Key_Enter || k === Qt.Key_Space) {
                    if (root.answer === 0)
                        root.run(root.confirming);
                    else
                        root.confirming = null;
                } else if (k === Qt.Key_N || k === Qt.Key_Escape)
                    root.confirming = null;
                else
                    return;
            } else {
                if (k === Qt.Key_Escape)
                    root.hide();
                else if (n > 0 && (k === Qt.Key_Right || k === Qt.Key_Tab || k === Qt.Key_L))
                    root.selected = (root.selected + 1) % n;
                else if (n > 0 && (k === Qt.Key_Left || k === Qt.Key_Backtab || k === Qt.Key_H))
                    root.selected = (root.selected + n - 1) % n;
                else if (k === Qt.Key_Return || k === Qt.Key_Enter || k === Qt.Key_Space)
                    root.choose(root.actions[root.selected]);
                else
                    return;
            }
            event.accepted = true;
        }
    }

    Panel {
        id: panel
        anchors.fill: parent

        Row {
            id: buttons
            x: root.gap
            y: root.gap
            spacing: root.gap

            Repeater {
                model: root.actions

                delegate: Item {
                    id: action
                    required property var modelData
                    required property int index
                    readonly property bool chosen: root.confirming ? root.confirming.id === modelData.id
                                                                   : index === root.selected

                    width: root.button
                    height: root.button

                    Rectangle {
                        anchors.fill: parent
                        radius: 2
                        color: action.chosen ? Theme.selection
                             : hover.hovered ? Qt.rgba(Theme.selection.r, Theme.selection.g, Theme.selection.b, 0.5)
                             : Theme.hardBackground
                    }
                    Rectangle {
                        visible: action.chosen
                        anchors { left: parent.left; right: parent.right; bottom: parent.bottom }
                        height: 3
                        color: action.modelData.ask ? Theme.red : Theme.aqua
                    }
                    Column {
                        anchors.centerIn: parent
                        spacing: 6
                        Text {
                            visible: Config.glyphs
                            anchors.horizontalCenter: parent.horizontalCenter
                            text: String.fromCodePoint(action.modelData.glyph)
                            color: action.modelData.ask ? Theme.red : Theme.aqua
                            font.family: Theme.font
                            font.pixelSize: Math.round(root.button * 0.32)
                        }
                        Text {
                            anchors.horizontalCenter: parent.horizontalCenter
                            text: action.modelData.label
                            color: Theme.foreground
                            font.family: Theme.font
                            font.pixelSize: Theme.fontSize + 1
                        }
                    }

                    HoverHandler {
                        id: hover
                        cursorShape: Qt.PointingHandCursor
                    }
                    TapHandler {
                        onTapped: {
                            root.selected = action.index;
                            root.choose(action.modelData);
                        }
                    }
                }
            }
        }

        // Always there, so the buttons above never move: the uptime, or the
        // question.
        Item {
            id: answerRow
            x: root.gap
            y: buttons.y + root.button + root.gap
            width: parent.width - 2 * root.gap
            height: Math.round(Theme.fontSize * 2.8)

            Text {
                anchors.centerIn: parent
                visible: !root.confirming
                text: root.uptimeText()
                color: Theme.dim
                font.family: Theme.font
                font.pixelSize: Theme.fontSize
            }

            RowLayout {
                anchors.fill: parent
                visible: !!root.confirming
                spacing: root.gap

                Text {
                    Layout.fillWidth: true
                    text: root.confirming ? root.confirming.label + " now?" : ""
                    color: Theme.foreground
                    elide: Text.ElideRight
                    font.family: Theme.font
                    font.pixelSize: Theme.fontSize + 1
                }

                Repeater {
                    model: root.confirming ? ["Yes, " + root.confirming.label.toLowerCase(), "Cancel"] : []

                    delegate: Item {
                        id: reply
                        required property string modelData
                        required property int index
                        Layout.fillHeight: true
                        implicitWidth: replyText.implicitWidth + 28

                        Rectangle {
                            anchors.fill: parent
                            radius: 2
                            color: reply.index === root.answer ? Theme.selection
                                 : replyHover.hovered ? Qt.rgba(Theme.selection.r, Theme.selection.g, Theme.selection.b, 0.5)
                                 : Theme.hardBackground
                        }
                        Text {
                            id: replyText
                            anchors.centerIn: parent
                            text: reply.modelData
                            color: reply.index === 0 ? Theme.red : Theme.foreground
                            font.family: Theme.font
                            font.pixelSize: Theme.fontSize + 1
                        }
                        HoverHandler {
                            id: replyHover
                            cursorShape: Qt.PointingHandCursor
                        }
                        TapHandler {
                            onTapped: {
                                if (reply.index === 0)
                                    root.run(root.confirming);
                                else
                                    root.confirming = null;
                            }
                        }
                    }
                }
            }
        }
    }
}
