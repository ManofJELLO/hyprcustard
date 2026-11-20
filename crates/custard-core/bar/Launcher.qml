import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Hyprland
import Quickshell.Io
import Quickshell.Wayland
import Quickshell.Widgets

// The app launcher: type to search the installed apps; Enter or a click
// starts one. The bar's launcher button opens it, and so does
// `qs ipc -p <this folder> call launcher toggle`, which hyprcustard's Bar
// page can put on the key that opened another launcher.
//
// Nothing in it moves unless typing moves it: the apps are read once each
// time it opens, the window keeps one size, and the pointer never scrolls
// the list or moves the selection. Up and Down (also Tab, Ctrl+N/P,
// Ctrl+J/K) move the selection; the list scrolls only to keep it in view.
PanelWindow {
    id: root

    property bool open: false
    // The apps as they were when it opened, and those that match.
    property var apps: []
    property var results: []
    property int selected: 0
    // How many times each app has been started from here, by desktop id.
    property var usage: ({})
    // When a click outside closed it: that click may have been on the bar's
    // button, which must not open it again straight away.
    property double clearedAt: 0

    readonly property int rows: Config.launcherRows
    readonly property int rowHeight: Math.round(Theme.fontSize * 2.8)
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
        root.screen = Config.menuScreen();
        readApps();
        if (search.text === "")
            refilter();
        else
            search.text = "";
        open = true;
        search.forceActiveFocus();
    }

    // Quickshell reads the installed apps in the background, from the
    // binding below, which starts it with the bar. They are copied when the
    // launcher opens, and again only if they arrive while it is open with
    // none listed, so nothing in the list moves while it is open.
    readonly property var installed: DesktopEntries.applications.values
    onInstalledChanged: {
        if (root.open && root.apps.length === 0) {
            readApps();
            refilter();
        }
    }

    function readApps() {
        root.apps = root.installed.filter(a => !a.noDisplay).map(a => ({
            entry: a,
            id: a.id,
            name: a.name,
            detail: a.genericName || a.comment || "",
            icon: a.icon,
            lname: (a.name || "").toLowerCase(),
            lgeneric: (a.genericName || "").toLowerCase(),
            lkeywords: (a.keywords || []).join(" ").toLowerCase(),
            lcomment: (a.comment || "").toLowerCase(),
            lid: (a.id || "").toLowerCase()
        }));
    }

    function hide() {
        open = false;
    }

    // Open with the search already typed.
    function showWith(query) {
        show();
        if (open)
            search.text = query;
    }

    function uses(a) {
        return root.usage[a.id] ?? 0;
    }

    // How well one search word matches a field: whole, start, start of a
    // word, anywhere, or (for the name) its letters in order.
    function fieldScore(text, w, weight, letters) {
        if (!text)
            return -1;
        if (text === w)
            return weight * 4;
        if (text.startsWith(w))
            return weight * 3;
        const i = text.indexOf(w);
        if (i > 0)
            return /[\s\-_.]/.test(text[i - 1]) ? weight * 2 : weight;
        if (letters) {
            let j = 0;
            for (let k = 0; k < text.length && j < w.length; k++)
                if (text[k] === w[j])
                    j++;
            if (j === w.length)
                return weight * 0.3;
        }
        return -1;
    }

    // Every word must match somewhere; -1 when one does not.
    function score(a, words) {
        let total = 0;
        for (const w of words) {
            const best = Math.max(fieldScore(a.lname, w, 10, true), fieldScore(a.lgeneric, w, 6, false),
                                  fieldScore(a.lkeywords, w, 5, false), fieldScore(a.lid, w, 4, false),
                                  fieldScore(a.lcomment, w, 2, false));
            if (best < 0)
                return -1;
            total += best;
        }
        return total;
    }

    function refilter() {
        const words = search.text.toLowerCase().split(/\s+/).filter(w => w !== "");
        let list;
        if (words.length === 0) {
            list = root.apps.slice().sort((a, b) => (uses(b) - uses(a)) || a.name.localeCompare(b.name));
        } else {
            list = [];
            for (const a of root.apps) {
                const s = score(a, words);
                if (s >= 0)
                    list.push({ app: a, s: s + Math.log2(1 + uses(a)) * 8 });
            }
            list.sort((x, y) => (y.s - x.s) || x.app.name.localeCompare(y.app.name));
            list = list.map(x => x.app);
        }
        root.results = list;
        root.selected = 0;
        view.positionViewAtBeginning();
    }

    function move(by) {
        if (root.results.length === 0)
            return;
        root.selected = Math.max(0, Math.min(root.results.length - 1, root.selected + by));
        view.positionViewAtIndex(root.selected, ListView.Contain);
    }

    function shellQuote(s) {
        return "'" + String(s).replace(/'/g, "'\\''") + "'";
    }

    function launch(a) {
        if (!a)
            return;
        const counts = Object.assign({}, root.usage);
        counts[a.id] = (counts[a.id] ?? 0) + 1;
        root.usage = counts;
        usageFile.setText(JSON.stringify(counts));
        const e = a.entry;
        if (e.runInTerminal && Config.terminal !== "") {
            // Field codes (%f, %U...) stand for files to open; there are none.
            const args = Array.from(e.command).filter(x => !/^%[a-zA-Z]$/.test(x)).map(shellQuote).join(" ");
            const cd = e.workingDirectory ? "cd " + shellQuote(e.workingDirectory) + " && " : "";
            Quickshell.execDetached(["sh", "-c", cd + "exec " + Config.terminal + " " + args]);
        } else {
            e.execute();
        }
        hide();
    }

    function iconFor(icon) {
        if (!icon)
            return Quickshell.iconPath("application-x-executable", true);
        if (icon.startsWith("/"))
            return "file://" + icon;
        return Quickshell.iconPath(icon, "application-x-executable");
    }

    FileView {
        id: usageFile
        path: Quickshell.statePath("launcher-usage.json")
        printErrors: false
        onLoaded: {
            try {
                root.usage = JSON.parse(text()) || {};
            } catch (e) {
                root.usage = {};
            }
        }
    }

    visible: open
    color: "transparent"
    implicitWidth: Config.launcherWidth + 2 * panel.pad
    implicitHeight: panel.pad * 2 + 2 * root.gap + root.rowHeight + root.gap + root.rows * root.rowHeight
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

    onVisibleChanged: if (visible) search.forceActiveFocus()

    HyprlandFocusGrab {
        windows: [root]
        active: root.visible
        // Hyprland ends the grab on a click outside the launcher.
        onCleared: {
            root.clearedAt = Date.now();
            root.hide();
        }
    }

    Panel {
        id: panel
        anchors.fill: parent

        ColumnLayout {
            anchors { fill: parent; margins: root.gap }
            spacing: root.gap

            Item {
                Layout.fillWidth: true
                implicitHeight: root.rowHeight

                // See-through like the card, flat or pressed in.
                Rectangle {
                    visible: !Config.neumorphic
                    anchors.fill: parent
                    radius: 2
                    color: Theme.glass(Theme.hardBackground)
                }
                Neu {
                    visible: Config.neumorphic
                    anchors.fill: parent
                    sunk: true
                    radius: 6
                }
                Text {
                    id: prompt
                    x: 12
                    anchors.verticalCenter: parent.verticalCenter
                    text: Config.glyphs ? String.fromCodePoint(0xf0349) : "\u203a"
                    color: Theme.aqua
                    font.family: Theme.font
                    font.pixelSize: Theme.fontSize + 2
                }
                TextInput {
                    id: search
                    anchors {
                        left: prompt.right
                        leftMargin: 10
                        right: parent.right
                        rightMargin: 12
                        verticalCenter: parent.verticalCenter
                    }
                    clip: true
                    color: Theme.foreground
                    selectionColor: Theme.aqua
                    selectedTextColor: Theme.hardBackground
                    font.family: Theme.font
                    font.pixelSize: Theme.fontSize + 2
                    onTextChanged: root.refilter()

                    Keys.onPressed: event => {
                        const k = event.key;
                        const ctrl = (event.modifiers & Qt.ControlModifier) !== 0;
                        if (k === Qt.Key_Escape)
                            root.hide();
                        else if (k === Qt.Key_Return || k === Qt.Key_Enter)
                            root.launch(root.results[root.selected]);
                        else if (k === Qt.Key_Down || k === Qt.Key_Tab || (ctrl && (k === Qt.Key_N || k === Qt.Key_J)))
                            root.move(1);
                        else if (k === Qt.Key_Up || k === Qt.Key_Backtab || (ctrl && (k === Qt.Key_P || k === Qt.Key_K)))
                            root.move(-1);
                        else if (k === Qt.Key_PageDown)
                            root.move(root.rows);
                        else if (k === Qt.Key_PageUp)
                            root.move(-root.rows);
                        else
                            return;
                        event.accepted = true;
                    }

                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        visible: search.text === ""
                        text: "Search apps"
                        color: Theme.dim
                        font: search.font
                    }
                }
            }

            ListView {
                id: view
                Layout.fillWidth: true
                Layout.preferredHeight: root.rows * root.rowHeight
                clip: true
                model: root.results
                boundsBehavior: Flickable.StopAtBounds
                // The selection is drawn by each row, so the view never
                // scrolls on its own to follow it.
                highlightFollowsCurrentItem: false

                delegate: Item {
                    id: row
                    required property var modelData
                    required property int index
                    readonly property bool chosen: index === root.selected

                    width: view.width
                    height: root.rowHeight

                    // The chosen row is lit (flat) or pressed in (neumorphic).
                    Rectangle {
                        anchors.fill: parent
                        radius: 2
                        color: row.chosen && !Config.neumorphic ? Theme.glass(Theme.selection)
                             : hover.hovered ? Qt.rgba(Theme.selection.r, Theme.selection.g, Theme.selection.b,
                                                       Config.opacity * 0.5)
                             : "transparent"
                    }
                    Rectangle {
                        visible: row.chosen && !Config.neumorphic
                        anchors { left: parent.left; top: parent.top; bottom: parent.bottom }
                        width: 3
                        color: Theme.aqua
                    }
                    Neu {
                        visible: row.chosen && Config.neumorphic
                        anchors.fill: parent
                        sunk: true
                        radius: 6
                    }
                    IconImage {
                        id: icon
                        visible: Config.launcherIcons
                        x: 12
                        anchors.verticalCenter: parent.verticalCenter
                        implicitSize: Math.round(root.rowHeight * 0.62)
                        source: root.iconFor(row.modelData.icon)
                        asynchronous: true
                    }
                    Text {
                        id: name
                        anchors {
                            left: Config.launcherIcons ? icon.right : parent.left
                            leftMargin: Config.launcherIcons ? 10 : 14
                            verticalCenter: parent.verticalCenter
                        }
                        width: Math.min(implicitWidth, row.width * 0.5)
                        elide: Text.ElideRight
                        text: row.modelData.name
                        color: Theme.foreground
                        font.family: Theme.font
                        font.pixelSize: Theme.fontSize + 1
                    }
                    Text {
                        anchors {
                            left: name.right
                            leftMargin: 12
                            right: parent.right
                            rightMargin: 12
                            verticalCenter: parent.verticalCenter
                        }
                        elide: Text.ElideRight
                        text: row.modelData.detail
                        color: Theme.dim
                        font.family: Theme.font
                        font.pixelSize: Theme.fontSize
                    }

                    HoverHandler {
                        id: hover
                        cursorShape: Qt.PointingHandCursor
                    }
                    TapHandler {
                        onTapped: root.launch(row.modelData)
                    }
                }

                Text {
                    anchors.centerIn: parent
                    visible: root.results.length === 0
                    text: root.apps.length === 0 ? "No apps found" : "No app matches"
                    color: Theme.dim
                    font.family: Theme.font
                    font.pixelSize: Theme.fontSize + 1
                }
            }
        }
    }
}
