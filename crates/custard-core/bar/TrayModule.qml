import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Services.SystemTray
import Quickshell.Widgets

// The system tray. Right-click an icon for its menu.
RowLayout {
    id: tray
    required property var bar
    property bool shown: true

    spacing: 10
    // The bar's right-hand group is anchored on the right, so new icons go on
    // the left: the ones already there stay where they are.
    layoutDirection: Qt.RightToLeft
    Layout.fillHeight: true
    Layout.leftMargin: 11
    visible: shown && SystemTray.items.values.length > 0

    // The tray menu that is open, so opening another closes it.
    property var openMenu: null

    Repeater {
        model: SystemTray.items

        delegate: Item {
            id: slot
            required property SystemTrayItem modelData
            implicitWidth: 16
            implicitHeight: 16

            IconImage {
                anchors.fill: parent
                source: slot.modelData.icon
                asynchronous: true
            }

            HoverHandler { id: hover }

            MouseArea {
                anchors.fill: parent
                acceptedButtons: Qt.LeftButton | Qt.RightButton | Qt.MiddleButton
                onClicked: mouse => {
                    const item = slot.modelData;
                    if (mouse.button === Qt.MiddleButton) item.secondaryActivate();
                    else if (mouse.button === Qt.LeftButton && !item.onlyMenu) item.activate();
                    else if (item.hasMenu) {
                        const was = menu.open;
                        if (tray.openMenu && tray.openMenu !== menu) tray.openMenu.open = false;
                        menu.open = !was;
                        return;
                    }
                    // Any other click in the tray closes an open menu.
                    if (tray.openMenu) tray.openMenu.open = false;
                }
                onWheel: wheel => slot.modelData.scroll(wheel.angleDelta.y, false)
            }

            // The menu opens just past the bar's edge, by the icon.
            TrayMenu {
                id: menu
                handle: slot.modelData.menu
                anchorItem: slot
                // From the icon to just past the bar's box.
                gap: (tray.height - slot.height) / 2 + 6
                grabWindows: [tray.bar]
                onDismissed: open = false
                onOpenChanged: {
                    if (open) tray.openMenu = menu;
                    else if (tray.openMenu === menu) tray.openMenu = null;
                }
            }

            Tooltip {
                target: slot
                hovered: hover.hovered && !menu.visible
                text: slot.modelData.tooltipTitle || slot.modelData.title || ""
            }
        }
    }
}
