import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import dev.hyprcustard.gui

// Icon and cursor themes, set everywhere each is read from: gsettings, GTK's
// settings.ini, qt5ct/qt6ct, X's default cursor, and Hyprland.
Item {
    id: page

    readonly property var view: App.state.icons || ({})
    readonly property var current: page.view.current || ({ icons: [], cursors: [] })
    readonly property string iconNow: page.firstValue(page.current.icons)
    readonly property string cursorNow: page.firstValue(page.current.cursors)
    readonly property int sizeNow: page.current.cursor_size || 24
    readonly property bool cursorBlocked: !page.view.cursorManaged
                                          && (page.view.cursorLines || []).length > 0

    Component.onCompleted: App.run("icons.refresh")
    onVisibleChanged: if (visible) App.run("icons.refresh")

    function firstValue(list) {
        return list && list.length > 0 ? list[0].value : ""
    }

    // "gsettings, GTK 3 and qt6ct: X" when they agree; each one when not.
    function summary(list) {
        if (!list || list.length === 0)
            return "Not set anywhere this app reads."
        var byValue = {}
        var order = []
        for (var i = 0; i < list.length; ++i) {
            var v = list[i].value
            if (!byValue[v]) {
                byValue[v] = []
                order.push(v)
            }
            byValue[v].push(list[i].place)
        }
        if (order.length === 1)
            return order[0] + ", in " + byValue[order[0]].join(", ")
        return order.map(function (v) { return byValue[v].join(", ") + ": " + v }).join("   ·   ")
    }

    function sizesFor(themeId) {
        var list = page.view.cursors || []
        for (var i = 0; i < list.length; ++i)
            if (list[i].id === themeId && list[i].sizes.length > 0)
                return list[i].sizes
        return [16, 24, 32, 48, 64]
    }

    PageScroll {
        id: scroll
        anchors.fill: parent

        ColumnLayout {
            width: Math.min(scroll.availableWidth, Theme.contentMax)
            x: Math.round((scroll.availableWidth - width) / 2)
            spacing: Theme.pad

            // ---------------------------------------------------------------
            // Icons
            // ---------------------------------------------------------------
            Card {
                Layout.fillWidth: true
                title: "Icon theme"
                subtitle: "Set for GTK and Qt apps at once: gsettings, GTK's settings.ini and "
                          + "qt5ct/qt6ct, where each is already in use. Apps that are open keep "
                          + "their icons until they restart."

                Text {
                    width: parent.width
                    text: "Now: " + page.summary(page.current.icons)
                    color: page.current.icons && page.current.icons.length > 0
                           && page.summary(page.current.icons).indexOf("·") >= 0 ? Theme.warn : Theme.fgDim
                    font.pixelSize: Theme.fontSize
                    wrapMode: Text.WordWrap
                }

                EmptyState {
                    width: parent.width
                    visible: page.view.scanned && (page.view.icons || []).length === 0
                    title: "No icon themes found"
                    body: "Icon themes are installed in ~/.local/share/icons, ~/.icons or /usr/share/icons."
                }

                Flow {
                    width: parent.width
                    spacing: Theme.gap

                    Repeater {
                        model: page.view.icons || []

                        Item {
                            id: tile

                            required property var modelData
                            readonly property bool chosen: tile.modelData.id === page.iconNow

                            width: 236
                            height: 96

                            Raised {
                                anchors.fill: parent
                                radius: Theme.radiusSmall
                                depth: Theme.depthSmall
                                sunk: tile.chosen || tileMouse.pressed
                            }

                            Column {
                                anchors.fill: parent
                                anchors.margins: Theme.gap
                                spacing: Theme.gapSmall

                                Text {
                                    width: parent.width
                                    text: tile.modelData.name
                                    color: tile.chosen ? Theme.accent : Theme.fg
                                    font.pixelSize: Theme.fontSize
                                    font.weight: tile.chosen ? Font.DemiBold : Font.Normal
                                    elide: Text.ElideRight
                                }
                                Row {
                                    spacing: 6

                                    Repeater {
                                        model: tile.modelData.previews

                                        Image {
                                            required property string modelData
                                            width: 28
                                            height: 28
                                            sourceSize: Qt.size(28, 28)
                                            fillMode: Image.PreserveAspectFit
                                            asynchronous: true
                                            smooth: true
                                            source: "file://" + modelData
                                        }
                                    }
                                }
                                Text {
                                    width: parent.width
                                    text: tile.modelData.dir
                                    color: Theme.muted
                                    font.pixelSize: Theme.fontSizeSmall
                                    elide: Text.ElideMiddle
                                }
                            }

                            MouseArea {
                                id: tileMouse
                                anchors.fill: parent
                                cursorShape: Qt.PointingHandCursor
                                onClicked: if (!tile.chosen) App.run("icons.setIcons", { theme: tile.modelData.id })
                            }
                        }
                    }
                }
            }

            // ---------------------------------------------------------------
            // Cursors
            // ---------------------------------------------------------------
            Card {
                Layout.fillWidth: true
                title: "Cursor"
                subtitle: "Changes at once in Hyprland, and is set for GTK, X apps and the next "
                          + "login. Qt apps draw the cursor Hyprland gives them."

                Text {
                    width: parent.width
                    text: "Now: " + page.summary(page.current.cursors)
                    color: page.summary(page.current.cursors).indexOf("·") >= 0 ? Theme.warn : Theme.fgDim
                    font.pixelSize: Theme.fontSize
                    wrapMode: Text.WordWrap
                }

                FieldRow {
                    width: parent.width
                    label: "Size"
                    hint: "In pixels. The sizes listed are the ones the current theme is drawn at."

                    Dropdown {
                        id: sizeBox
                        width: 140
                        model: page.sizesFor(page.cursorNow).map(function (s) { return String(s) })
                        currentIndex: page.sizesFor(page.cursorNow).indexOf(page.sizeNow)
                        displayText: String(page.sizeNow)
                        enabled: page.cursorNow !== ""
                        onActivated: function (index) {
                            App.run("icons.setCursor", {
                                theme: page.cursorNow,
                                size: page.sizesFor(page.cursorNow)[index]
                            })
                        }
                    }
                }

                // The user's own lines set it at login too.
                Column {
                    width: parent.width
                    visible: page.cursorBlocked
                    spacing: Theme.gapSmall

                    Text {
                        width: parent.width
                        text: "Your Hyprland config sets the cursor itself, so a cursor picked here "
                              + "lasts only until the next login. Take these lines over and they are "
                              + "commented out, with a block of hyprcustard's setting the same thing; "
                              + "Give them back puts them back exactly."
                        color: Theme.warn
                        font.pixelSize: Theme.fontSize
                        wrapMode: Text.WordWrap
                    }
                    Repeater {
                        model: page.view.cursorLines || []

                        Text {
                            required property var modelData
                            width: parent.width
                            text: "line " + modelData.number + ":  " + modelData.text
                                  + (modelData.movable ? "" : "   (shares its line with other code)")
                            color: modelData.movable ? Theme.fgDim : Theme.error
                            font.family: Theme.monoFamily
                            font.pixelSize: Theme.fontSizeSmall
                            elide: Text.ElideRight
                        }
                    }
                    PillButton {
                        text: "Take them over"
                        primary: true
                        enabled: page.cursorNow !== ""
                        onClicked: App.run("icons.takeOver", { theme: page.cursorNow, size: page.sizeNow })
                    }
                }

                RowLayout {
                    width: parent.width
                    visible: (page.view.cursorMoved || []).length > 0
                    spacing: Theme.gap

                    Text {
                        Layout.fillWidth: true
                        text: App.plural((page.view.cursorMoved || []).length, "line")
                              + " of your own are commented out in its place."
                        color: Theme.muted
                        font.pixelSize: Theme.fontSizeSmall
                    }
                    PillButton {
                        text: "Give them back"
                        onClicked: App.confirm("Give your cursor lines back?",
                                               "The cursor block is taken out and your own lines are "
                                               + "uncommented, exactly as they were.",
                                               "Give them back", false,
                                               function () { App.run("icons.giveBack") })
                    }
                }

                EmptyState {
                    width: parent.width
                    visible: page.view.scanned && (page.view.cursors || []).length === 0
                    title: "No cursor themes found"
                    body: "Cursor themes are installed in ~/.icons, ~/.local/share/icons or /usr/share/icons."
                }

                Flow {
                    width: parent.width
                    spacing: Theme.gap

                    Repeater {
                        model: page.view.cursors || []

                        Item {
                            id: ctile

                            required property var modelData
                            readonly property bool chosen: ctile.modelData.id === page.cursorNow

                            width: 236
                            height: 96

                            Raised {
                                anchors.fill: parent
                                radius: Theme.radiusSmall
                                depth: Theme.depthSmall
                                sunk: ctile.chosen || cmouse.pressed
                            }

                            Column {
                                anchors.fill: parent
                                anchors.margins: Theme.gap
                                spacing: Theme.gapSmall

                                Text {
                                    width: parent.width
                                    text: ctile.modelData.name
                                    color: ctile.chosen ? Theme.accent : Theme.fg
                                    font.pixelSize: Theme.fontSize
                                    font.weight: ctile.chosen ? Font.DemiBold : Font.Normal
                                    elide: Text.ElideRight
                                }
                                Row {
                                    spacing: 10
                                    height: 28

                                    Repeater {
                                        model: ctile.modelData.previews

                                        Image {
                                            required property string modelData
                                            width: 28
                                            height: 28
                                            fillMode: Image.PreserveAspectFit
                                            smooth: true
                                            cache: false
                                            source: "file://" + modelData
                                        }
                                    }
                                    Text {
                                        visible: ctile.modelData.previews.length === 0
                                        text: ctile.modelData.hyprcursor ? "hyprcursor only" : "no preview"
                                        color: Theme.muted
                                        font.pixelSize: Theme.fontSizeSmall
                                        anchors.verticalCenter: parent.verticalCenter
                                    }
                                }
                                Text {
                                    width: parent.width
                                    text: ctile.modelData.dir
                                    color: Theme.muted
                                    font.pixelSize: Theme.fontSizeSmall
                                    elide: Text.ElideMiddle
                                }
                            }

                            MouseArea {
                                id: cmouse
                                anchors.fill: parent
                                cursorShape: Qt.PointingHandCursor
                                onClicked: if (!ctile.chosen) App.run("icons.setCursor", {
                                    theme: ctile.modelData.id,
                                    size: page.sizeNow
                                })
                            }
                        }
                    }
                }
            }
        }
    }
}
