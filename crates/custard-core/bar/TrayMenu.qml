import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Hyprland
import Quickshell.Widgets

// A tray item's menu drawn in QML with Theme's colors, so it follows a color
// scheme change at once. (QsMenuAnchor shows a native Qt menu, whose colors
// come from qt6ct and are read only when Quickshell starts.)
//
// A submenu opens beside its entry on click, as another TrayMenu.
//
// Set `open` rather than `visible`. A top-level menu keeps its entries loaded
// even while closed, so it opens at once at its final size; a submenu waits
// until the app has sent its entries. Either way it never grows or slides
// while shown (growing left artifacts behind with Steam).
//
// A click outside closes it through Hyprland's focus grab. The bar is in the
// grab (`grabWindows`), so a click on another tray icon reaches that icon.
PopupWindow {
    id: root

    property QsMenuHandle handle: null
    property Item anchorItem: null
    property bool isSub: false
    // Space between the anchor and a top-level menu (past the bar's edge).
    property real gap: 14

    property bool open: false
    // Windows besides the menus that a click may land in without closing them.
    property var grabWindows: []
    // This menu and its open submenus.
    readonly property var chain: [root].concat(sub.item ? sub.item.chain : [])

    // Chosen an entry, clicked outside or pressed Escape: every level closes.
    signal dismissed()

    property QsMenuEntry subEntry: null
    property Item subRow: null

    anchor.item: anchorItem
    anchor.rect.x: isSub ? (anchorItem ? anchorItem.width : 0) : 0
    anchor.rect.y: isSub ? -pad
                         : Config.bottom ? -gap
                         : (anchorItem ? anchorItem.height + gap : 0)
    anchor.rect.width: isSub ? 0 : (anchorItem ? anchorItem.width : 0)
    anchor.rect.height: 0
    anchor.edges: isSub ? (Edges.Top | Edges.Left) : Config.bottom ? Edges.Top : Edges.Bottom
    anchor.gravity: isSub ? (Edges.Bottom | Edges.Right) : Config.bottom ? Edges.Top : Edges.Bottom

    readonly property real pad: 4

    color: "transparent"
    implicitWidth: card.width + 2
    implicitHeight: card.height + 2
    visible: false

    onOpenChanged: {
        if (open) {
            if (count > 0) {
                visible = true;
            } else {
                settle.restart();
                giveUp.restart();
            }
        } else {
            settle.stop();
            giveUp.stop();
            visible = false;
        }
    }
    onVisibleChanged: {
        if (visible) {
            // Grab once the menu's surface exists, or the grab misses it.
            if (!isSub) Qt.callLater(() => { if (root.visible) grab.active = true; });
            return;
        }
        grab.active = false;
        subEntry = null;
        subRow = null;
    }

    QsMenuOpener {
        id: opener
        menu: root.isSub ? (root.open ? root.handle : null) : root.handle
    }

    HyprlandFocusGrab {
        id: grab
        windows: root.grabWindows.concat(root.chain)
        // Hyprland ends the grab on a click outside every window in it.
        onCleared: if (root.open) root.dismissed()
    }

    readonly property int count: opener.children ? opener.children.values.length : 0
    onCountChanged: if (open && !visible) settle.restart()

    // Shown once the entries have stopped arriving for a moment.
    Timer {
        id: settle
        interval: 40
        onTriggered: if (root.open && root.count > 0) root.visible = true
    }
    // An app that sends no entries: give up rather than stay "open" unseen.
    Timer {
        id: giveUp
        interval: 1500
        onTriggered: if (root.open && !root.visible) root.open = false
    }

    // Dbusmenu labels mark a mnemonic with "_" and a literal underscore with "__".
    function label(t) { return (t || "").replace(/_(_)?/g, "$1"); }

    Rectangle {
        anchors.fill: card
        anchors.margins: -1
        radius: 3
        color: "transparent"
        border.width: 1
        border.color: Qt.rgba(Theme.shadow.r, Theme.shadow.g, Theme.shadow.b, 0.6)
    }

    Rectangle {
        id: card
        x: 1; y: 1
        width: Math.max(list.implicitWidth, 120) + 2 * root.pad
        height: list.implicitHeight + 2 * root.pad
        radius: 2
        color: Theme.hardBackground

        focus: true
        Keys.onEscapePressed: root.dismissed()

        ColumnLayout {
            id: list
            x: root.pad; y: root.pad
            width: card.width - 2 * root.pad
            spacing: 0

            Repeater {
                model: opener.children

                delegate: Item {
                    id: row
                    required property QsMenuEntry modelData
                    readonly property QsMenuEntry entry: modelData
                    readonly property bool isOpenSub: root.subEntry === entry

                    Layout.fillWidth: true
                    implicitWidth: entry.isSeparator ? 0 : line.implicitWidth + 16
                    implicitHeight: entry.isSeparator ? 9 : 24

                    Rectangle {
                        visible: row.entry.isSeparator
                        anchors.verticalCenter: parent.verticalCenter
                        x: 6; width: parent.width - 12; height: 1
                        color: Qt.rgba(Theme.foreground.r, Theme.foreground.g, Theme.foreground.b, 0.15)
                    }

                    Rectangle {
                        visible: !row.entry.isSeparator && row.entry.enabled
                                 && (rowHover.hovered || row.isOpenSub)
                        anchors.fill: parent
                        radius: 2
                        color: Qt.rgba(Theme.blue.r, Theme.blue.g, Theme.blue.b, 0.25)
                    }

                    RowLayout {
                        id: line
                        visible: !row.entry.isSeparator
                        anchors { fill: parent; leftMargin: 8; rightMargin: 8 }
                        spacing: 8
                        opacity: row.entry.enabled ? 1 : 0.4

                        // Check box or radio button.
                        Rectangle {
                            visible: row.entry.buttonType !== QsMenuButtonType.None
                            implicitWidth: 12; implicitHeight: 12
                            radius: row.entry.buttonType === QsMenuButtonType.RadioButton ? 6 : 2
                            color: "transparent"
                            border.width: 1
                            border.color: Theme.foreground
                            Rectangle {
                                anchors { fill: parent; margins: 3 }
                                radius: parent.radius > 2 ? width / 2 : 1
                                visible: row.entry.checkState === Qt.Checked
                                color: Theme.blue
                            }
                        }

                        IconImage {
                            visible: row.entry.icon !== ""
                            implicitSize: 16
                            source: row.entry.icon
                            asynchronous: true
                        }

                        Text {
                            Layout.fillWidth: true
                            text: root.label(row.entry.text)
                            color: Theme.foreground
                            font.family: Theme.font
                            font.pixelSize: Theme.fontSize
                        }

                        Text {
                            visible: row.entry.hasChildren
                            text: "›"
                            color: Theme.foreground
                            font.family: Theme.font
                            font.pixelSize: Theme.fontSize + 2
                        }
                    }

                    HoverHandler { id: rowHover; enabled: !row.entry.isSeparator }

                    MouseArea {
                        anchors.fill: parent
                        enabled: !row.entry.isSeparator && row.entry.enabled
                        onClicked: {
                            if (row.entry.hasChildren) {
                                const again = row.isOpenSub;
                                // Close the old submenu first, so a new one is
                                // built for this entry rather than reshaped.
                                root.subEntry = null;
                                root.subRow = null;
                                if (!again) {
                                    const e = row.entry, r = row;
                                    Qt.callLater(() => { root.subEntry = e; root.subRow = r; });
                                }
                            } else {
                                row.entry.triggered();
                                root.dismissed();
                            }
                        }
                    }
                }
            }
        }
    }

    Loader {
        id: sub
        active: root.visible && root.subEntry !== null && root.subRow !== null
        source: "TrayMenu.qml"
        onLoaded: {
            item.isSub = true;
            item.handle = Qt.binding(() => root.subEntry);
            item.anchorItem = Qt.binding(() => root.subRow);
            // A submenu closing because another entry was chosen here is not
            // a dismissal (subEntry is already null then).
            item.dismissed.connect(() => { if (root.subEntry !== null) root.dismissed(); });
            item.open = true;
        }
    }
}
