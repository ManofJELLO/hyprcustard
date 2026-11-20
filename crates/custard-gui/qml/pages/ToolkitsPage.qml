import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import dev.hyprcustard.gui

// GTK 3 and Kvantum themes, each shown first as a preview drawn by the
// toolkit itself, offscreen, then set with one button. Only the toolkits
// that are installed get a card.
Item {
    id: page

    readonly property var view: App.state.toolkits || ({})
    readonly property var found: App.state.toolkitsFound || ({})
    readonly property var gtk: page.view.gtk || ({ themes: [], pins: [] })
    readonly property var qt: page.view.qt || ({ themes: [], styles: [] })

    // The theme on show in each preview: picked by a click, the one in use
    // until then.
    property string gtkPick: ""
    property string qtPick: ""
    // Recolor the Kvantum theme with the scheme: what is in use now, until
    // the switch is moved.
    property var recolorChoice: null
    readonly property bool recolor: page.qt.scheme ? (page.recolorChoice === null
                                                      ? page.qt.recolored === true
                                                      : page.recolorChoice)
                                                   : false

    // The Kvantum theme in use, by the name the list has: a recolored copy
    // counts as the theme it was made from.
    readonly property string qtNow: page.qt.recolored ? (page.qt.base || "") : (page.qt.now || "")
    readonly property bool qtInUse: page.qtPick !== "" && page.qtPick === page.qtNow
                                    && page.recolor === (page.qt.recolored === true)

    // The last request for each preview, and what came back for it.
    property string gtkRequest: ""
    property string qtRequest: ""
    property var gtkShown: ({ path: "", error: "", busy: false })
    property var qtShown: ({ path: "", error: "", busy: false })

    Component.onCompleted: App.run("toolkits.refresh")
    onVisibleChanged: if (visible) App.run("toolkits.refresh")

    // Once the lists are in, show what is in use.
    onViewChanged: {
        if (!page.view.scanned)
            return
        if (page.gtkPick === "" && page.found.gtk && page.gtk.now)
            page.previewGtk(page.gtk.now)
        if (page.qtPick === "" && page.found.qt && page.qtNow !== "")
            page.previewQt(page.qtNow)
    }

    function previewGtk(id) {
        page.gtkPick = id
        page.gtkRequest = JSON.stringify({ kind: "gtk", theme: id })
        page.gtkShown = { path: page.gtkShown.path, error: "", busy: true }
        App.backend.toolkitPreview(page.gtkRequest)
    }

    function previewQt(id) {
        page.qtPick = id
        page.qtRequest = JSON.stringify({ kind: "qt", theme: id, recolor: page.recolor })
        page.qtShown = { path: page.qtShown.path, error: "", busy: true }
        App.backend.toolkitPreview(page.qtRequest)
    }

    function find(list, id) {
        for (var i = 0; i < (list || []).length; ++i)
            if (list[i].id === id)
                return list[i]
        return null
    }

    Connections {
        target: App.backend

        // Only the newest request for each counts; an older one finishing
        // late must not replace it.
        function onToolkitPreviewReady(request, path, error) {
            if (request === page.gtkRequest)
                page.gtkShown = { path: path, error: error, busy: false }
            else if (request === page.qtRequest)
                page.qtShown = { path: path, error: error, busy: false }
        }
    }

    PageScroll {
        id: scroll
        anchors.fill: parent

        ColumnLayout {
            width: Math.min(scroll.availableWidth, Theme.contentMax)
            x: Math.round((scroll.availableWidth - width) / 2)
            spacing: Theme.pad

            SaveNote {
                Layout.fillWidth: true
                label: "Saves at once"
                text: "Clicking a theme only previews it. Use it, and it is written to your apps' "
                      + "settings at once. The Save button at the top is only for the Hyprland pages."
            }

            EmptyState {
                Layout.fillWidth: true
                visible: !page.found.any
                title: "Neither GTK 3 nor Kvantum is installed"
                body: "This page shows when one is: GTK 3 for GTK themes (the gtk3 package), "
                      + "Kvantum for Qt themes (the kvantum package)."
            }

            // ---------------------------------------------------------------
            // GTK
            // ---------------------------------------------------------------
            Card {
                Layout.fillWidth: true
                visible: page.found.gtk === true
                title: "GTK 3 apps"
                subtitle: "Set in gsettings and GTK's settings.ini. GTK 4 apps built on libadwaita "
                          + "take no theme; they follow the Colors page instead."

                Text {
                    width: parent.width
                    text: "Now: " + (page.gtk.now || "Adwaita")
                          + (page.gtk.preferDark ? ", its dark version where it has one" : "")
                    color: Theme.fgDim
                    font.pixelSize: Theme.fontSize
                    wrapMode: Text.WordWrap
                }

                Column {
                    width: parent.width
                    visible: (page.gtk.pins || []).length > 0
                    spacing: Theme.gapSmall

                    Text {
                        width: parent.width
                        text: "Your Hyprland config sets the GTK theme with gsettings each time "
                              + "Hyprland starts or reloads, so a theme used here goes back to its "
                              + "value then. Take it out and the line is commented out with a note."
                        color: Theme.warn
                        font.pixelSize: Theme.fontSize
                        wrapMode: Text.WordWrap
                    }
                    Repeater {
                        model: page.gtk.pins || []

                        Text {
                            required property var modelData
                            width: parent.width
                            text: modelData.place + ":  gtk-theme = " + modelData.value
                            color: Theme.fgDim
                            font.family: Theme.monoFamily
                            font.pixelSize: Theme.fontSizeSmall
                            elide: Text.ElideMiddle
                        }
                    }
                    PillButton {
                        text: "Take it out"
                        primary: true
                        onClicked: App.run("toolkits.takePins", { theme: page.gtk.now })
                    }
                }

                PreviewRow {
                    width: parent.width
                    shown: page.gtkShown
                    theme: page.find(page.gtk.themes, page.gtkPick)
                    inUse: page.gtkPick !== "" && page.gtkPick === page.gtk.now
                    notes: {
                        var t = page.find(page.gtk.themes, page.gtkPick)
                        if (!t)
                            return []
                        var out = []
                        if (t.calls_itself)
                            out.push("Calls itself " + t.calls_itself + ".")
                        if (t.dark_version)
                            out.push(page.gtk.preferDark ? "Shown in its dark version, as you prefer dark."
                                                         : "Has a dark version too.")
                        if (t.follows)
                            out.push("Draws with GTK's color names, so your color scheme reaches GTK 3 apps under it.")
                        else if (page.gtk.colorsOn)
                            out.push("Has colors of its own: GTK 3 apps keep them whatever your color "
                                     + "scheme is. libadwaita apps still follow the scheme.")
                        if (t.gtk4)
                            out.push("Has GTK 4 files, which this page does not set up.")
                        return out
                    }
                    onUse: App.run("toolkits.useGtk", { theme: page.gtkPick })
                }

                EmptyState {
                    width: parent.width
                    visible: page.view.scanned && (page.gtk.themes || []).length === 0
                    title: "No GTK 3 themes found"
                    body: "GTK themes are installed in ~/.local/share/themes, ~/.themes or /usr/share/themes."
                }

                ThemeTiles {
                    width: parent.width
                    themes: page.gtk.themes || []
                    current: page.gtk.now || ""
                    picked: page.gtkPick
                    onPick: function (id) { page.previewGtk(id) }
                }
            }

            // ---------------------------------------------------------------
            // Qt
            // ---------------------------------------------------------------
            Card {
                Layout.fillWidth: true
                visible: page.found.qt === true
                title: "Qt apps: Kvantum themes"
                subtitle: "Set in Kvantum's settings, with Kvantum made the style in qt6ct and qt5ct. "
                          + "Qt apps that are open keep their look until they restart."

                Text {
                    width: parent.width
                    text: "Now: " + (page.qt.now === "" || page.qt.now === undefined ? "Kvantum's built-in theme"
                                     : page.qt.recolored ? (page.qt.base || "?") + ", recolored with your color scheme"
                                                         : page.qt.now)
                    color: Theme.fgDim
                    font.pixelSize: Theme.fontSize
                    wrapMode: Text.WordWrap
                }

                Text {
                    width: parent.width
                    visible: page.view.scanned === true && page.qt.usesKvantum === false
                    text: page.qt.styleOverride
                          ? "QT_STYLE_OVERRIDE is set to " + page.qt.styleOverride + ", which Qt follows "
                            + "over qt6ct, so Qt apps do not draw with Kvantum. The Colors page can take it out."
                          : page.qt.platform === "qt6ct" || page.qt.platform === "qt5ct"
                            ? "Qt apps are not drawing with Kvantum yet: using a theme here makes it qt6ct's style."
                            : "Qt apps are not reading qt6ct's settings (QT_QPA_PLATFORMTHEME is "
                              + (page.qt.platform ? page.qt.platform : "not set") + "), so they will not "
                              + "draw with Kvantum until it is set to qt6ct in your Hyprland environment."
                    color: Theme.warn
                    font.pixelSize: Theme.fontSize
                    wrapMode: Text.WordWrap
                }

                RowLayout {
                    width: parent.width
                    visible: !!page.qt.scheme
                    spacing: Theme.gap

                    Toggle {
                        checked: page.recolor
                        onToggled: {
                            page.recolorChoice = checked
                            if (page.qtPick !== "")
                                page.previewQt(page.qtPick)
                        }
                    }
                    Text {
                        Layout.fillWidth: true
                        text: "Recolor it with your color scheme, " + page.qt.scheme
                              + ". The theme's shapes and shading stay; its colors follow the Colors page."
                        color: Theme.fgDim
                        font.pixelSize: Theme.fontSize
                        wrapMode: Text.WordWrap
                    }
                }

                PreviewRow {
                    width: parent.width
                    shown: page.qtShown
                    theme: page.find(page.qt.themes, page.qtPick)
                    inUse: page.qtInUse
                    notes: {
                        var t = page.find(page.qt.themes, page.qtPick)
                        if (!t)
                            return []
                        var out = []
                        if (page.recolor)
                            out.push("Shown recolored with " + page.qt.scheme + ".")
                        out.push(t.mine ? "One of your own, in " + t.dir + "." : "Installed in " + t.dir + ".")
                        return out
                    }
                    onUse: App.run("toolkits.useKvantum", { theme: page.qtPick, recolor: page.recolor })
                }

                EmptyState {
                    width: parent.width
                    visible: page.view.scanned && (page.qt.themes || []).length === 0
                    title: "No Kvantum themes found"
                    body: "Kvantum themes are installed in ~/.config/Kvantum or /usr/share/Kvantum."
                }

                ThemeTiles {
                    width: parent.width
                    themes: page.qt.themes || []
                    current: page.qtNow
                    picked: page.qtPick
                    onPick: function (id) { page.previewQt(id) }
                }
            }
        }
    }

    // The preview, a fixed size so nothing below it moves while one is
    // drawn, beside the picked theme's name, colors and Use button.
    component PreviewRow: GridLayout {
        id: row

        property var shown: ({ path: "", error: "", busy: false })
        property var theme: null
        property bool inUse: false
        property var notes: []
        signal use()

        columns: width >= 820 ? 2 : 1
        columnSpacing: Theme.pad
        rowSpacing: Theme.gap

        Item {
            Layout.preferredWidth: 520
            Layout.preferredHeight: 300
            Layout.maximumWidth: row.width
            Layout.alignment: Qt.AlignTop

            Inset {
                anchors.fill: parent
                radius: Theme.radiusSmall
            }
            Image {
                id: shot
                anchors.fill: parent
                anchors.margins: 4
                fillMode: Image.PreserveAspectFit
                asynchronous: true
                cache: false
                smooth: true
                source: row.shown.path ? "file://" + row.shown.path : ""
                opacity: row.shown.busy ? 0.4 : 1
            }
            Text {
                anchors.centerIn: parent
                width: parent.width - 2 * Theme.pad
                horizontalAlignment: Text.AlignHCenter
                visible: text !== ""
                text: row.shown.error ? row.shown.error
                      : row.shown.busy ? "Drawing the preview…"
                      : shot.status !== Image.Ready ? "Click a theme below to see it here" : ""
                color: row.shown.error ? Theme.error : Theme.muted
                font.pixelSize: Theme.fontSize
                wrapMode: Text.WordWrap
            }
        }

        // A fixed height, so a theme with more to say does not push the
        // tiles below down under the pointer.
        Column {
            Layout.fillWidth: true
            Layout.preferredHeight: row.columns === 1 ? 180 : 300
            Layout.alignment: Qt.AlignTop
            spacing: Theme.gap
            clip: true

            Text {
                width: parent.width
                text: row.theme ? (row.theme.name || row.theme.id) : ""
                color: Theme.fg
                font.pixelSize: Theme.fontSizeLarge
                font.weight: Font.DemiBold
                elide: Text.ElideRight
            }
            Swatches {
                colors: row.theme ? row.theme.swatches : []
                size: 22
            }
            Repeater {
                model: row.notes

                Text {
                    required property string modelData
                    width: parent.width
                    text: modelData
                    color: Theme.fgDim
                    font.pixelSize: Theme.fontSize
                    wrapMode: Text.WordWrap
                }
            }
            PillButton {
                visible: row.theme !== null
                primary: !row.inUse
                enabled: !row.inUse
                text: row.inUse ? "In use" : "Use " + (row.theme ? (row.theme.name || row.theme.id) : "")
                onClicked: row.use()
            }
        }
    }

    // One tile per theme. The one in use is pressed in; the one previewed
    // is outlined.
    component ThemeTiles: Flow {
        id: tiles

        property var themes: []
        property string current: ""
        property string picked: ""
        signal pick(string id)

        spacing: Theme.tileGap

        Repeater {
            model: tiles.themes

            Item {
                id: tile

                required property var modelData
                readonly property bool chosen: tile.modelData.id === tiles.current
                readonly property bool shownNow: tile.modelData.id === tiles.picked

                width: 200
                height: 68

                Raised {
                    anchors.fill: parent
                    radius: Theme.radiusSmall
                    depth: Theme.depthSmall
                    sunk: tile.chosen || mouse.pressed
                }
                Rectangle {
                    anchors.fill: parent
                    radius: Theme.radiusSmall
                    color: "transparent"
                    border.width: 2
                    border.color: Theme.accent
                    visible: tile.shownNow
                }

                Column {
                    anchors.fill: parent
                    anchors.margins: Theme.gap
                    spacing: Theme.gapSmall

                    Text {
                        width: parent.width
                        text: tile.modelData.name || tile.modelData.id
                        color: tile.chosen ? Theme.accent : Theme.fg
                        font.pixelSize: Theme.fontSize
                        font.weight: tile.chosen ? Font.DemiBold : Font.Normal
                        elide: Text.ElideRight
                    }
                    Swatches {
                        colors: tile.modelData.swatches
                        size: 14
                    }
                }

                MouseArea {
                    id: mouse
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    onClicked: tiles.pick(tile.modelData.id)
                }
            }
        }
    }

    component Swatches: Row {
        id: sw

        property var colors: []
        property int size: 14

        spacing: 4
        height: sw.size

        Repeater {
            model: sw.colors || []

            Rectangle {
                required property string modelData
                width: sw.size
                height: sw.size
                radius: sw.size / 4
                color: modelData
                border.width: 1
                border.color: Theme.border
            }
        }
    }
}
