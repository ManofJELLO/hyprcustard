import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Hyprland
import Quickshell.Io
import Quickshell.Wayland

// The power menu: the buttons the Bar page chooses from Lock, Log out,
// Suspend, Reboot and Shut down, in a row or a column. The bar's power
// button opens it, and so does `qs ipc -p <this folder> call power toggle`.
// Log out, Reboot and Shut down ask first, in the row under the buttons,
// which is always there so the buttons never move: Enter or Y says yes, Esc
// or N goes back. The arrow keys and Tab move between buttons.
PanelWindow {
    id: root

    property bool open: false
    property int selected: 0
    // The action waiting for a yes, and which answer is picked (0 yes, 1 no).
    property var confirming: null
    property int answer: 0
    property double clearedAt: 0
    property real uptime: 0

    readonly property var all: [
        { id: "lock", label: "Lock", glyph: 0xf033e, ask: false },
        { id: "logout", label: "Log out", glyph: 0xf0343, ask: true },
        { id: "suspend", label: "Suspend", glyph: 0xf04b2, ask: false },
        { id: "reboot", label: "Reboot", glyph: 0xf0709, ask: true },
        { id: "shutdown", label: "Shut down", glyph: 0xf0425, ask: true }
    ]
    // The chosen ones that have a command.
    readonly property var actions: root.all.filter(a => Config.powerItems.indexOf(a.id) >= 0
                                                        && (Config.power[a.id] ?? "") !== "")
    readonly property bool icons: Config.glyphs && Config.powerIcons
    readonly property bool column: Config.powerColumn

    // A button: square in a row; wide and short in a column.
    readonly property int unit: Math.round(Theme.fontSize * 8)
    readonly property int buttonWidth: root.column ? Math.round(root.unit * 1.9) : root.unit
    readonly property int buttonHeight: root.column ? Math.round(Theme.fontSize * 3.6)
                                                   : root.icons ? root.unit : Math.round(root.unit * 0.6)
    // Raised buttons stand far enough apart that their shadows never meet.
    readonly property int gap: Config.neumorphic ? 2 * Theme.reach(Theme.depthSmall) : 8
    readonly property int count: Math.max(1, root.actions.length)
    readonly property int buttonsWidth: root.column ? root.buttonWidth
                                                    : root.count * root.buttonWidth + (root.count - 1) * root.gap
    readonly property int buttonsHeight: root.column ? root.count * root.buttonHeight + (root.count - 1) * root.gap
                                                     : root.buttonHeight
    readonly property int answerHeight: Math.round(Theme.fontSize * 2.8)
    // Wide enough for the question and its two answers.
    readonly property int innerWidth: Math.max(root.buttonsWidth, Math.round(Theme.fontSize * 30))

    function toggle() {
        if (open)
            hide();
        else
            show();
    }

    function show() {
        if (open || Date.now() - clearedAt < 300)
            return;
        root.screen = Config.menuScreen();
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
    implicitWidth: panel.pad * 2 + root.gap * 2 + root.innerWidth
    implicitHeight: panel.pad * 2 + root.gap * 3 + root.buttonsHeight + root.answerHeight
    // Kept out of the bar's space, so a menu at the top opens under it.
    exclusionMode: ExclusionMode.Normal
    anchors {
        top: Config.menuTop
        bottom: Config.menuBottom
        left: Config.menuLeft
        right: Config.menuRight
    }
    margins {
        top: Config.menuMargin
        bottom: Config.menuMargin
        left: Config.menuMargin
        right: Config.menuMargin
    }
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
            const next = k === Qt.Key_Right || k === Qt.Key_Down || k === Qt.Key_Tab || k === Qt.Key_L || k === Qt.Key_J;
            const back = k === Qt.Key_Left || k === Qt.Key_Up || k === Qt.Key_Backtab || k === Qt.Key_H || k === Qt.Key_K;
            const enter = k === Qt.Key_Return || k === Qt.Key_Enter || k === Qt.Key_Space;
            if (root.confirming) {
                if (next || back)
                    root.answer = 1 - root.answer;
                else if (k === Qt.Key_Y)
                    root.run(root.confirming);
                else if (enter) {
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
                else if (n > 0 && next)
                    root.selected = (root.selected + 1) % n;
                else if (n > 0 && back)
                    root.selected = (root.selected + n - 1) % n;
                else if (enter)
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

        Grid {
            id: buttons
            x: root.gap + (root.innerWidth - root.buttonsWidth) / 2
            y: root.gap
            columns: root.column ? 1 : root.count
            spacing: root.gap

            Repeater {
                model: root.actions

                delegate: Item {
                    id: action
                    required property var modelData
                    required property int index
                    readonly property bool chosen: root.confirming ? root.confirming.id === modelData.id
                                                                   : index === root.selected
                    readonly property color tint: modelData.ask ? Theme.red : Theme.aqua

                    width: root.buttonWidth
                    height: root.buttonHeight

                    // Flat: see-through like the card, lit when chosen.
                    Rectangle {
                        visible: !Config.neumorphic
                        anchors.fill: parent
                        radius: 2
                        color: action.chosen ? Theme.glass(Theme.selection)
                             : hover.hovered ? Qt.rgba(Theme.selection.r, Theme.selection.g, Theme.selection.b,
                                                       Config.opacity * 0.6)
                             : Theme.glass(Theme.hardBackground)
                    }
                    Rectangle {
                        visible: action.chosen && !Config.neumorphic
                        anchors { left: parent.left; right: parent.right; bottom: parent.bottom }
                        height: 3
                        color: action.tint
                    }
                    // Neumorphic: raised, and pressed in when chosen.
                    Neu {
                        visible: Config.neumorphic
                        anchors.fill: parent
                        sunk: action.chosen || hover.hovered
                        depth: Theme.depthSmall
                        radius: 8
                    }

                    // A row's buttons stack the icon over the label; a
                    // column's put it beside.
                    Grid {
                        anchors.centerIn: parent
                        columns: root.column ? 2 : 1
                        spacing: root.column ? 10 : 6
                        horizontalItemAlignment: Grid.AlignHCenter
                        verticalItemAlignment: Grid.AlignVCenter
                        Text {
                            visible: root.icons
                            text: String.fromCodePoint(action.modelData.glyph)
                            color: action.tint
                            font.family: Theme.font
                            font.pixelSize: root.column ? Theme.fontSize + 6 : Math.round(root.unit * 0.32)
                        }
                        Text {
                            text: action.modelData.label
                            color: action.chosen && Config.neumorphic ? action.tint : Theme.foreground
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

        Text {
            anchors.centerIn: buttons
            visible: root.actions.length === 0
            text: "No buttons chosen"
            color: Theme.dim
            font.family: Theme.font
            font.pixelSize: Theme.fontSize + 1
        }

        // Always there, so the buttons above never move: the uptime, or the
        // question.
        Item {
            id: answerRow
            x: root.gap
            y: buttons.y + root.buttonsHeight + root.gap
            width: root.innerWidth
            height: root.answerHeight

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
                        Layout.topMargin: Config.neumorphic ? 4 : 0
                        Layout.bottomMargin: Config.neumorphic ? 4 : 0
                        implicitWidth: replyText.implicitWidth + 28

                        Rectangle {
                            visible: !Config.neumorphic
                            anchors.fill: parent
                            radius: 2
                            color: reply.index === root.answer ? Theme.glass(Theme.selection)
                                 : replyHover.hovered ? Qt.rgba(Theme.selection.r, Theme.selection.g, Theme.selection.b,
                                                                Config.opacity * 0.6)
                                 : Theme.glass(Theme.hardBackground)
                        }
                        Neu {
                            visible: Config.neumorphic
                            anchors.fill: parent
                            sunk: reply.index === root.answer || replyHover.hovered
                            depth: Theme.depthSmall
                            radius: 6
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
