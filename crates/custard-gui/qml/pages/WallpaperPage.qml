import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import QtQuick.Dialogs
import dev.hyprcustard.gui

// Wallpaper: browse your wallpapers and put one up with the program you
// already use.
//
// Nothing here replaces the user's wallpaper program; the backend drives
// it. What is on screen comes from a scan run on a thread when the page
// opens, the files from the `wallpaperLibrary` query, and the previews are
// made by ffmpeg in the background, arriving one signal at a time.
Item {
    id: page

    readonly property var wall: App.state.wallpaper || ({})
    readonly property var choice: wall.settings || ({})
    readonly property var monitors: wall.monitors || []
    readonly property var using: wall.using || null
    readonly property var startup: wall.startup || ({})
    readonly property var programs: (wall.programs || []).filter(function (p) { return p.installed })
    readonly property bool busy: wall.busy === true
    // A start line sharing its line with other code cannot be handed over.
    readonly property bool stuckLines: (startup.lines || []).some(function (l) { return !l.movable })
    // Folders as text, so the library is read again only when they change.
    readonly property string foldersKey: JSON.stringify(wall.folders || [])

    // Which monitor a pick goes on; empty for all of them. Not saved.
    property string target: ""
    property var library: ({ groups: [], total: 0 })
    // Groups showing every file rather than the first [perGroup].
    property var expanded: ({})
    readonly property int perGroup: 60

    // Previews finished since the library was read, by file. Bindings read
    // it through `readyVersion`, which a timer bumps a few times a second
    // rather than once per preview.
    property var readyMap: ({})
    property int readyVersion: 0
    property int pending: 0

    function scan() {
        if (App.backend)
            App.backend.wallpaperScan()
    }

    function load() {
        var lib = App.queryJson("wallpaperLibrary", {})
        if (lib.error)
            lib = { groups: [], total: 0, error: lib.error }
        page.library = lib
        page.readyMap = ({})
        page.readyVersion++
        page.makeThumbs()
    }

    // Ask for the previews still missing: what is on screen first, then the
    // files in page order.
    function makeThumbs() {
        if (!App.backend)
            return
        var want = []
        for (var m = 0; m < page.monitors.length; m++) {
            var w = page.monitors[m].wallpaper
            if (w && w.file)
                want.push(w.file)
        }
        var groups = page.library.groups || []
        for (var g = 0; g < groups.length; g++) {
            var files = groups[g].files
            for (var f = 0; f < files.length; f++) {
                if (!files[f].ready)
                    want.push(files[f].path)
            }
        }
        page.pending = want.length
        App.backend.wallpaperThumbs(JSON.stringify(want))
    }

    function ready(file, readyAtLoad) {
        return readyAtLoad || (page.readyVersion >= 0 && page.readyMap[file] === true)
    }

    function shownOn(path) {
        var on = []
        for (var i = 0; i < page.monitors.length; i++) {
            var w = page.monitors[i].wallpaper
            if (w && (w.file === path || w.path === path))
                on.push(page.monitors[i].name)
        }
        return on
    }

    function canShow(video, folder) {
        if (!page.using)
            return false
        return folder ? page.using.folders : (!video || page.using.videos)
    }

    function put(path) {
        if (App.backend)
            App.backend.wallpaperSet(JSON.stringify({ path: path, monitor: page.target }))
    }

    function set(changes) {
        App.run("wallpaper.set", changes)
    }

    onVisibleChanged: if (page.visible) page.scan()
    Component.onCompleted: if (page.visible) page.scan()
    onFoldersKeyChanged: if (page.wall.scanned) page.load()

    Connections {
        target: App.backend

        function onWallpaperThumbReady(file, thumb) {
            page.readyMap[file] = true
            page.pending = Math.max(0, page.pending - 1)
            flush.start()
        }
    }

    Timer {
        id: flush
        interval: 250
        onTriggered: page.readyVersion++
    }

    FolderDialog {
        id: folderDialog
        title: "Add a folder of wallpapers"
        onAccepted: App.run("wallpaper.addFolder", { path: folderDialog.selectedFolder.toString() })
    }

    ScrollView {
        id: scroll
        anchors.fill: parent
        clip: true
        contentWidth: availableWidth

        ColumnLayout {
            width: Math.min(scroll.availableWidth, Theme.contentMax)
            x: Math.round((scroll.availableWidth - width) / 2)
            spacing: Theme.pad

            // -----------------------------------------------------------
            Card {
                Layout.fillWidth: true
                title: "Wallpaper"
                subtitle: "Pick one from your folders below. It goes up with the wallpaper "
                          + "program you already use, keeping your own options for it."

                Text {
                    visible: !page.wall.scanned
                    text: "Looking at the desktop…"
                    color: Theme.muted
                    font.pixelSize: Theme.fontSize
                }

                Flow {
                    visible: page.wall.scanned === true && page.monitors.length > 0
                    width: parent.width
                    spacing: Theme.gap

                    Repeater {
                        model: page.monitors

                        Rectangle {
                            id: now
                            required property var modelData
                            readonly property var shown: now.modelData.wallpaper
                            readonly property bool chosen: page.target === now.modelData.name
                            width: 236
                            height: 182
                            radius: Theme.radiusSmall
                            color: Theme.surface
                            border.width: now.chosen ? 2 : 1
                            border.color: now.chosen ? Theme.accent : Theme.border

                            Rectangle {
                                id: nowFrame
                                x: 8
                                y: 8
                                width: parent.width - 16
                                height: Math.round(nowFrame.width * 9 / 16)
                                radius: 3
                                color: Theme.bgAlt
                                clip: true

                                Image {
                                    anchors.fill: parent
                                    visible: !!now.shown && !!now.shown.thumb
                                             && page.ready(now.shown.file, false)
                                    source: visible ? "file://" + now.shown.thumb : ""
                                    fillMode: Image.PreserveAspectCrop
                                    asynchronous: true
                                    cache: false
                                }
                                Text {
                                    anchors.centerIn: parent
                                    visible: !now.shown
                                    text: "Nothing hyprcustard can read"
                                    color: Theme.muted
                                    font.pixelSize: Theme.fontSizeSmall
                                }
                            }
                            Text {
                                x: 8
                                y: nowFrame.y + nowFrame.height + 6
                                width: parent.width - 16
                                text: now.modelData.name
                                color: Theme.fg
                                font.pixelSize: Theme.fontSize
                                font.weight: Font.DemiBold
                                elide: Text.ElideRight
                            }
                            Text {
                                x: 8
                                y: nowFrame.y + nowFrame.height + 26
                                width: parent.width - 16
                                text: now.shown ? (now.shown.fileName || now.shown.display) + " · " + now.shown.program : ""
                                color: Theme.muted
                                font.pixelSize: Theme.fontSizeSmall
                                elide: Text.ElideMiddle
                            }
                            MouseArea {
                                id: nowMouse
                                anchors.fill: parent
                                hoverEnabled: true
                                cursorShape: Qt.PointingHandCursor
                                onClicked: page.target = now.chosen ? "" : now.modelData.name
                            }
                            Tip {
                                visible: nowMouse.containsMouse && page.monitors.length > 1
                                delay: 400
                                text: now.chosen ? "Picks go on " + now.modelData.name + " only. Click again for every monitor."
                                                 : "Put picks on " + now.modelData.name + " only"
                            }
                        }
                    }
                }

                Text {
                    visible: (page.wall.unreadable || []).length > 0
                    width: parent.width
                    wrapMode: Text.WordWrap
                    text: "Also drawing the background: " + (page.wall.unreadable || []).join(", ")
                          + ". hyprcustard cannot tell what it shows."
                    color: Theme.muted
                    font.pixelSize: Theme.fontSizeSmall
                }

                FieldRow {
                    visible: page.monitors.length > 1
                    width: parent.width
                    label: "Put it on"
                    Dropdown {
                        width: 260
                        model: ["Every monitor"].concat(page.monitors.map(function (m) { return m.name }))
                        currentIndex: Math.max(0, page.monitors.map(function (m) { return m.name }).indexOf(page.target) + 1)
                        onActivated: function (index) {
                            page.target = index === 0 ? "" : page.monitors[index - 1].name
                        }
                    }
                }

                FieldRow {
                    width: parent.width
                    label: "Show it with"
                    hint: {
                        if (page.programs.length === 0)
                            return "No wallpaper program is installed. Install one of awww, swww, "
                                   + "hyprpaper, swaybg or mpvpaper — mpvpaper is the one that plays videos."
                        var running = (page.wall.programs || []).filter(function (p) { return p.running })
                        if (page.using && running.length > 0 && !running.some(function (p) { return p.id === page.using.id }))
                            return "The next pick moves every monitor to " + page.using.id + " and stops "
                                   + running.map(function (p) { return p.id }).join(" and ") + "."
                        if (page.using && page.using.id === "mpvpaper")
                            return "mpvpaper plays videos and whole folders, and keeps the options you started it with."
                        return page.using && !page.using.videos ? page.using.id + " shows images and GIFs, not videos." : ""
                    }
                    Dropdown {
                        width: 260
                        enabled: page.programs.length > 0
                        model: page.programs.map(function (p) {
                            return p.id + (p.running ? " (running)" : "")
                        })
                        currentIndex: page.using ? Math.max(0, page.programs.map(function (p) { return p.id }).indexOf(page.using.id)) : -1
                        onActivated: function (index) {
                            page.set({ program: page.programs[index].id })
                        }
                    }
                }

                FieldRow {
                    width: parent.width
                    label: "Fit"
                    hint: page.using && page.using.id === "mpvpaper"
                          && (page.wall.programs || []).some(function (p) { return p.id === "mpvpaper" && p.running })
                          ? "mpvpaper keeps the options it was started with, so this applies once it is started fresh."
                          : ""
                    Dropdown {
                        width: 260
                        model: ["Fill the screen", "Fit inside it", "Stretch to it"]
                        currentIndex: Math.max(0, ["fill", "fit", "stretch"].indexOf(page.choice.fit || "fill"))
                        onActivated: function (index) {
                            page.set({ fit: ["fill", "fit", "stretch"][index] })
                        }
                    }
                }

                Toggle {
                    text: page.wall.colorsFollow
                          ? "Colors follow the wallpaper (Colors page), so each pick recolors the desktop at once"
                          : "Recolor the desktop to match each pick, with the Colors page's style and apps"
                    enabled: !page.wall.colorsFollow
                    checked: page.wall.colorsFollow === true || page.choice.recolor === true
                    onToggled: page.set({ recolor: checked })
                }

                Text {
                    visible: page.busy
                    text: "Putting it up…"
                    color: Theme.accent
                    font.pixelSize: Theme.fontSize
                }
            }

            // -----------------------------------------------------------
            Card {
                Layout.fillWidth: true
                title: "At login"
                subtitle: page.startup.managed
                          ? "hyprcustard starts the wallpaper at login, from a block at the end of "
                            + page.startup.config + ", and updates it with every pick."
                          : (page.startup.lines || []).length > 0
                            ? "A pick here lasts until you log out: " + page.startup.config
                              + " starts the wallpaper at login with its own line."
                            : "Nothing in " + page.startup.config + " starts a wallpaper, so a pick "
                              + "here lasts until you log out."

                Repeater {
                    model: page.startup.managed ? (page.startup.moved || []) : (page.startup.lines || [])
                    Text {
                        required property var modelData
                        width: parent ? parent.width : 0
                        text: (page.startup.managed ? "commented out, line " : "line ") + modelData.number + ":  " + modelData.text
                        color: page.startup.managed ? Theme.muted : modelData.movable === false ? Theme.warn : Theme.fg
                        font.family: Theme.monoFamily
                        font.pixelSize: Theme.fontSizeSmall
                        elide: Text.ElideRight
                    }
                }

                Text {
                    visible: (page.startup.services || []).length > 0
                    width: parent.width
                    wrapMode: Text.WordWrap
                    text: "systemd also starts " + (page.startup.services || []).join(", ")
                          + " at login. hyprcustard leaves services alone; if it shows a different "
                          + "wallpaper, turn it off with systemctl --user disable."
                    color: Theme.warn
                    font.pixelSize: Theme.fontSizeSmall
                }

                Text {
                    visible: !page.startup.managed && page.stuckLines
                    width: parent.width
                    wrapMode: Text.WordWrap
                    text: "The line in yellow has other code on it, so hyprcustard will not comment "
                          + "it out. Put the start command on a line of its own to hand it over."
                    color: Theme.warn
                    font.pixelSize: Theme.fontSizeSmall
                }

                RowLayout {
                    spacing: Theme.gapSmall
                    PillButton {
                        visible: !page.startup.managed
                        text: (page.startup.lines || []).length > 0 ? "Start what is on screen instead" : "Start it at login"
                        enabled: !page.stuckLines && (page.wall.programs || []).some(function (p) { return p.running })
                        tooltip: (page.startup.lines || []).length > 0
                                 ? "Comment out the line above (marked, so Give it back restores it) and add a "
                                   + "block that starts what is on screen now. A backup is made first."
                                 : "Add a block that starts what is on screen now. A backup is made first."
                        onClicked: App.run("wallpaper.keepAtLogin", {})
                    }
                    PillButton {
                        visible: page.startup.managed === true
                        text: "Give it back"
                        tooltip: "Remove hyprcustard's block and put your own lines back as they were"
                        onClicked: App.run("wallpaper.giveBack", {})
                    }
                }
            }

            // -----------------------------------------------------------
            Card {
                Layout.fillWidth: true
                title: "Your wallpapers"
                subtitle: page.wall.ffmpeg === false
                          ? "Previews need ffmpeg, which is not installed."
                          : page.pending > 0 ? "Making previews… " + page.pending + " to go."
                          : (page.library.total || 0) + " files" + (page.library.capped ? " (the first ones only)" : "")
                            + ". Click one to put it up."

                Flow {
                    width: parent.width
                    spacing: Theme.gapSmall

                    Repeater {
                        model: page.wall.folders || []
                        Rectangle {
                            id: chip
                            required property var modelData
                            height: 28
                            width: chipText.implicitWidth + 40
                            radius: 14
                            color: Theme.surface
                            border.width: 1
                            border.color: Theme.border
                            Text {
                                id: chipText
                                x: 12
                                anchors.verticalCenter: parent.verticalCenter
                                text: chip.modelData.display
                                color: Theme.fg
                                font.pixelSize: Theme.fontSizeSmall
                            }
                            Text {
                                anchors.right: parent.right
                                anchors.rightMargin: 10
                                anchors.verticalCenter: parent.verticalCenter
                                text: "×"
                                color: chipX.containsMouse ? Theme.error : Theme.muted
                                font.pixelSize: Theme.fontSize
                                MouseArea {
                                    id: chipX
                                    anchors.fill: parent
                                    anchors.margins: -6
                                    hoverEnabled: true
                                    cursorShape: Qt.PointingHandCursor
                                    onClicked: App.run("wallpaper.removeFolder", { path: chip.modelData.path })
                                }
                            }
                            HoverHandler {
                                id: chipHover
                            }
                            Tip {
                                visible: chipHover.hovered
                                delay: 400
                                text: chip.modelData.found ? "Found here because " + (chip.modelData.display)
                                                             + " is a usual place for wallpapers, or your wallpaper program reads it"
                                                           : "Added by you"
                            }
                        }
                    }
                    PillButton {
                        compact: true
                        text: "Add a folder…"
                        onClicked: folderDialog.open()
                    }
                    PillButton {
                        compact: true
                        text: "Look again"
                        tooltip: "Read the folders and the desktop again"
                        onClicked: {
                            page.scan()
                            page.load()
                        }
                    }
                }

                EmptyState {
                    visible: page.wall.scanned === true && (page.library.groups || []).length === 0
                    width: parent.width
                    title: "No wallpapers found"
                    body: "Add the folder you keep them in. Images and videos are listed, "
                          + "including those in its subfolders."
                }

                Repeater {
                    model: page.library.groups || []

                    Column {
                        id: group
                        required property var modelData
                        readonly property bool open: page.expanded[group.modelData.dir] === true
                        readonly property var shownFiles: group.open ? group.modelData.files
                                                                     : group.modelData.files.slice(0, page.perGroup)
                        readonly property var playingOn: page.shownOn(group.modelData.dir)
                        width: parent.width
                        spacing: Theme.gapSmall

                        RowLayout {
                            width: parent.width
                            spacing: Theme.gap
                            Text {
                                text: group.modelData.name
                                color: Theme.fg
                                font.pixelSize: Theme.fontSizeLarge
                                font.weight: Font.DemiBold
                            }
                            Text {
                                text: group.modelData.files.length + (group.modelData.files.length === 1 ? " file" : " files")
                                color: Theme.muted
                                font.pixelSize: Theme.fontSizeSmall
                            }
                            Badge {
                                visible: group.playingOn.length > 0
                                text: "Playing on " + group.playingOn.join(", ")
                            }
                            Item {
                                Layout.fillWidth: true
                            }
                            PillButton {
                                compact: true
                                visible: !!page.using && page.using.folders
                                text: "Play the whole folder"
                                enabled: !page.busy
                                tooltip: "mpvpaper plays every file here in turn, with your options"
                                onClicked: page.put(group.modelData.dir)
                            }
                        }

                        Flow {
                            width: parent.width
                            spacing: Theme.gap

                            Repeater {
                                model: group.shownFiles

                                Rectangle {
                                    id: tile
                                    required property var modelData
                                    readonly property var on: page.shownOn(tile.modelData.path)
                                    readonly property bool showable: page.canShow(tile.modelData.video, false)
                                    width: 192
                                    height: 140
                                    radius: Theme.radiusSmall
                                    color: tileMouse.containsMouse && tile.showable ? Theme.surfaceHi : Theme.surface
                                    border.width: tile.on.length > 0 ? 2 : 1
                                    border.color: tile.on.length > 0 ? Theme.accent : Theme.border
                                    opacity: tile.showable ? 1 : 0.45

                                    Rectangle {
                                        id: frame
                                        x: 6
                                        y: 6
                                        width: parent.width - 12
                                        height: 101
                                        radius: 3
                                        color: Theme.bgAlt
                                        clip: true
                                        Image {
                                            anchors.fill: parent
                                            visible: page.ready(tile.modelData.path, tile.modelData.ready)
                                            source: visible && tile.modelData.thumb ? "file://" + tile.modelData.thumb : ""
                                            sourceSize.width: 360
                                            fillMode: Image.PreserveAspectCrop
                                            asynchronous: true
                                        }
                                        Badge {
                                            visible: tile.modelData.video
                                            anchors.right: parent.right
                                            anchors.bottom: parent.bottom
                                            anchors.margins: 5
                                            text: "video"
                                            solid: true
                                        }
                                    }
                                    Text {
                                        x: 8
                                        y: frame.y + frame.height + 7
                                        width: parent.width - 16
                                        text: tile.modelData.name
                                        color: Theme.fg
                                        font.pixelSize: Theme.fontSizeSmall
                                        elide: Text.ElideMiddle
                                    }
                                    MouseArea {
                                        id: tileMouse
                                        anchors.fill: parent
                                        hoverEnabled: true
                                        cursorShape: tile.showable && !page.busy ? Qt.PointingHandCursor : Qt.ArrowCursor
                                        onClicked: if (tile.showable && !page.busy) page.put(tile.modelData.path)
                                    }
                                    Tip {
                                        visible: tileMouse.containsMouse
                                        delay: 500
                                        text: !tile.showable
                                              ? (page.using ? page.using.id + " cannot play videos. Choose mpvpaper under Show it with." : "No wallpaper program is installed")
                                              : tile.on.length > 0 ? tile.modelData.name + ", on " + tile.on.join(", ")
                                              : "Put " + tile.modelData.name + " on " + (page.target || "every monitor")
                                    }
                                }
                            }
                        }

                        PillButton {
                            compact: true
                            visible: group.modelData.files.length > page.perGroup
                            text: group.open ? "Show fewer" : "Show all " + group.modelData.files.length
                            onClicked: {
                                var e = Object.assign({}, page.expanded)
                                e[group.modelData.dir] = !group.open
                                page.expanded = e
                            }
                        }
                    }
                }
            }
        }
    }
}
