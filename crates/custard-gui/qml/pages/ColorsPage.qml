import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import QtQuick.Dialogs
import dev.hyprcustard.gui

// Colors: one color scheme for the whole desktop.
//
// The scheme comes from a named palette, the wallpaper, or a color picked
// here, and is written for each app turned on below. Building one only
// reads — the preview shows it before anything changes — and Apply writes
// the files and asks the apps to reload.
Item {
    id: page

    readonly property var colors: App.state.colors || ({})
    readonly property var choice: colors.settings || ({})
    readonly property string source: choice.source || ""
    readonly property string mode: source.indexOf("scheme:") === 0 ? "palette"
                                   : source === "wallpaper" ? "wallpaper" : "color"
    readonly property var built: colors.built || null
    readonly property bool building: colors.building === true
    readonly property var scheme: (built && built.ok) ? built.scheme : null
    readonly property var roles: page.scheme ? page.scheme.roles : ({})
    readonly property var targets: colors.targets || []
    readonly property var installedTargets: page.targets.filter(function (t) { return t.installed })
    readonly property var missingTargets: page.targets.filter(function (t) { return !t.installed })

    function set(changes) {
        App.run("colors.set", changes)
    }

    function role(name) {
        return page.roles[name] || "#000000"
    }

    // The targets turned off, with `id` switched to `on`.
    // A scheme color as the pairing list names it.
    function choiceLabel(name) {
        var hues = { base08: "red", base09: "orange", base0A: "yellow", base0B: "green",
                     base0C: "cyan", base0D: "blue", base0E: "purple", base0F: "brown" }
        if (hues[name])
            return name + " (" + hues[name] + ")"
        return String(name).replace(/_/g, " ")
    }

    function setTarget(id, on) {
        var off = (page.choice.off || []).filter(function (x) { return x !== id })
        if (!on)
            off.push(id)
        page.set({ off: off })
    }

    function build() {
        if (App.backend)
            App.backend.colorsBuild()
    }

    onVisibleChanged: if (page.visible) page.build()
    Component.onCompleted: if (page.visible) page.build()

    ColorDialog {
        id: seedDialog
        title: "Pick a color to build a scheme from"
        onAccepted: page.set({ source: "color:" + App.hyprHex(App.argb(seedDialog.selectedColor)).substr(0, 6) })
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
                title: "Colors"
                subtitle: "One color scheme for your desktop: Hyprland's borders and the apps "
                          + "you turn on below. Choose a palette, or build one from your "
                          + "wallpaper or from any color. Nothing changes until you press Apply."

                RowLayout {
                    width: parent.width
                    spacing: Theme.gapSmall

                    Repeater {
                        model: [
                            { id: "palette", label: "A palette" },
                            { id: "wallpaper", label: "The wallpaper" },
                            { id: "color", label: "A color" }
                        ]
                        PillButton {
                            required property var modelData
                            text: modelData.label
                            primary: page.mode === modelData.id
                            onClicked: {
                                if (page.mode === modelData.id)
                                    return
                                if (modelData.id === "palette")
                                    page.set({ source: "scheme:gruvbox-dark" })
                                else if (modelData.id === "wallpaper")
                                    page.set({ source: "wallpaper" })
                                else
                                    page.set({ source: "color:" + (page.scheme ? page.scheme.source.substr(1) : "6750a4") })
                            }
                        }
                    }
                    Item {
                        Layout.fillWidth: true
                    }
                    Text {
                        visible: !!page.colors.applied
                        text: "Applied now: " + (page.colors.applied ? page.colors.applied.name : "")
                        color: Theme.muted
                        font.pixelSize: Theme.fontSizeSmall
                    }
                    PillButton {
                        text: page.scheme ? "Apply " + page.scheme.name : "Apply"
                        primary: true
                        enabled: !!page.scheme && !page.building
                        tooltip: "Write this scheme for every app turned on below, and reload them"
                        onClicked: App.run("colors.apply", {})
                    }
                }

                // --- a palette ---
                Flow {
                    visible: page.mode === "palette"
                    width: parent.width
                    spacing: Theme.gap

                    Repeater {
                        model: page.colors.palettes || []

                        Rectangle {
                            id: tile
                            required property var modelData
                            readonly property bool chosen: page.source === "scheme:" + tile.modelData.id
                            width: 196
                            height: 62
                            radius: Theme.radiusSmall
                            color: tile.modelData.swatch[0]
                            border.width: tile.chosen ? 2 : 1
                            border.color: tile.chosen ? Theme.accent : Theme.border

                            Text {
                                x: 10
                                y: 8
                                width: parent.width - 20
                                text: tile.modelData.name
                                elide: Text.ElideRight
                                color: tile.modelData.swatch[2]
                                font.pixelSize: Theme.fontSize
                                font.bold: tile.chosen
                            }
                            Row {
                                x: 10
                                y: 34
                                spacing: 3
                                Repeater {
                                    model: tile.modelData.swatch.slice(3)
                                    Rectangle {
                                        required property var modelData
                                        width: 22
                                        height: 16
                                        radius: 3
                                        color: modelData
                                    }
                                }
                            }
                            MouseArea {
                                anchors.fill: parent
                                cursorShape: Qt.PointingHandCursor
                                onClicked: page.set({ source: "scheme:" + tile.modelData.id })
                            }
                        }
                    }
                }
                RowLayout {
                    visible: page.mode === "palette"
                    width: parent.width
                    spacing: Theme.gap
                    Text {
                        Layout.fillWidth: true
                        text: "Add any base16 scheme as a .yaml file in " + (page.colors.paletteDir || "")
                              + " — most popular palettes are published in that format."
                        wrapMode: Text.WordWrap
                        color: Theme.muted
                        font.pixelSize: Theme.fontSizeSmall
                    }
                    PillButton {
                        compact: true
                        text: "Reload palettes"
                        onClicked: App.run("colors.reloadPalettes", {})
                    }
                }

                // --- the wallpaper ---
                Text {
                    visible: page.mode === "wallpaper" && page.colors.ffmpeg === false
                    width: parent.width
                    text: "ffmpeg is needed to read the wallpaper. Install it from your "
                          + "distribution's packages, then press Look again."
                    wrapMode: Text.WordWrap
                    color: Theme.warn
                    font.pixelSize: Theme.fontSize
                }
                RowLayout {
                    visible: page.mode === "wallpaper"
                    width: parent.width
                    spacing: Theme.gap

                    Text {
                        Layout.fillWidth: true
                        text: page.building ? "Reading the wallpaper…"
                              : (page.built && page.built.ok) ? page.built.from
                              : (page.built ? page.built.error : "")
                        wrapMode: Text.WordWrap
                        color: (page.built && !page.built.ok) ? Theme.warn : Theme.fgDim
                        font.pixelSize: Theme.fontSize
                    }
                    PillButton {
                        compact: true
                        text: "Look again"
                        enabled: !page.building
                        tooltip: "Read the wallpaper again — after it has changed, say"
                        onClicked: App.run("colors.rebuild", {})
                    }
                }
                FieldRow {
                    visible: page.mode === "wallpaper" && !!page.built && page.built.ok
                             && (page.built.candidates || []).length > 0
                    width: parent.width
                    label: "Its colors"
                    hint: "The wallpaper's strongest colors, best first. Click one to build "
                          + "the scheme from it instead."
                    Row {
                        spacing: Theme.gapSmall
                        Repeater {
                            model: (page.built && page.built.candidates) || []
                            ColorButton {
                                required property var modelData
                                value: modelData
                                tooltip: modelData + (page.scheme && page.scheme.source === modelData
                                                      ? " — the scheme is built from this" : "")
                                border.width: page.scheme && page.scheme.source === modelData ? 2 : 1
                                onClicked: page.set({ source: "color:" + modelData.substr(1) })
                            }
                        }
                    }
                }
                FieldRow {
                    visible: page.mode === "wallpaper"
                    width: parent.width
                    label: "Build on"
                    Dropdown {
                        width: 260
                        model: ["Its most prominent color", "Its most colorful color"]
                        currentIndex: page.choice.pick === "colorful" ? 1 : 0
                        onActivated: function (index) {
                            page.set({ pick: index === 1 ? "colorful" : "prominent" })
                        }
                    }
                }
                FieldRow {
                    id: monitorRow
                    readonly property var monitors: (page.built && page.built.monitors) || []
                    visible: page.mode === "wallpaper" && monitorRow.monitors.length > 1
                    width: parent.width
                    label: "Monitor"
                    hint: "Whose wallpaper counts, when they differ."
                    Dropdown {
                        width: 260
                        model: ["The first one"].concat(monitorRow.monitors)
                        currentIndex: Math.max(0, monitorRow.monitors.indexOf(page.choice.monitor) + 1)
                        onActivated: function (index) {
                            page.set({ monitor: index === 0 ? "" : monitorRow.monitors[index - 1] })
                        }
                    }
                }
                Toggle {
                    visible: page.mode === "wallpaper" && !!page.built && page.built.ok
                             && !!page.built.wallpaper && page.built.wallpaper.folder
                    text: "Use every file in the folder, not just the one showing"
                    checked: page.choice.whole_folder === true
                    onToggled: page.set({ whole_folder: checked })
                }

                // --- a color ---
                FieldRow {
                    visible: page.mode === "color"
                    width: parent.width
                    label: "Seed color"
                    RowLayout {
                        spacing: Theme.gap
                        ColorButton {
                            value: "#ff" + page.source.replace("color:", "").replace("#", "")
                            tooltip: "Pick a color"
                            onClicked: {
                                seedDialog.selectedColor = "#" + page.source.replace("color:", "").replace("#", "")
                                seedDialog.open()
                            }
                        }
                        LineEdit {
                            id: seedEdit
                            Layout.preferredWidth: 120
                            mono: true
                            text: page.source.replace("color:", "").replace("#", "")
                            onCommit: function (v) {
                                seedEdit.text = Qt.binding(function () {
                                    return page.source.replace("color:", "").replace("#", "")
                                })
                                var hex = v.trim().replace("#", "")
                                if (hex !== "" && "color:" + hex !== page.source)
                                    page.set({ source: "color:" + hex })
                            }
                        }
                    }
                }

                // --- how a seed becomes a scheme ---
                FieldRow {
                    id: styleRow
                    visible: page.mode !== "palette"
                    width: parent.width
                    label: "Style"
                    readonly property var styles: page.colors.styles || []
                    readonly property int at: {
                        for (var i = 0; i < styleRow.styles.length; ++i)
                            if (styleRow.styles[i].id === page.choice.style)
                                return i
                        return 0
                    }
                    hint: styleRow.styles.length ? styleRow.styles[styleRow.at].hint : ""
                    Dropdown {
                        width: 260
                        model: styleRow.styles
                        textRole: "label"
                        currentIndex: styleRow.at
                        onActivated: function (index) {
                            page.set({ style: styleRow.styles[index].id })
                        }
                    }
                }
                Toggle {
                    visible: page.mode !== "palette"
                    text: "Dark"
                    checked: page.choice.dark !== false
                    onToggled: page.set({ dark: checked })
                }
                LabeledSlider {
                    visible: page.mode !== "palette"
                    width: parent.width
                    label: "Contrast"
                    from: -1
                    to: 1
                    stepSize: 0.1
                    value: page.choice.contrast || 0
                    markValue: 0
                    onSettled: function (v) { page.set({ contrast: Math.round(v * 10) / 10 }) }
                }
            }

            // -----------------------------------------------------------
            Card {
                id: followCard
                readonly property var follow: page.colors.follow || ({})
                readonly property var status: followCard.follow.status || null
                visible: page.mode === "wallpaper" || followCard.follow.on === true
                Layout.fillWidth: true
                title: "Follow the wallpaper"
                subtitle: "Keeps the colors matching the wallpaper as it changes, from now on and "
                          + "after every login. A small program, custard-watch, sleeps until the "
                          + "wallpaper changes — it uses about 5 MB and no CPU while it waits — and "
                          + "holds a change back while anything is fullscreen. It follows what you "
                          + "last applied here, so trying other colors on this page leaves it alone."

                Text {
                    visible: followCard.follow.installed === false
                    width: parent.width
                    text: "custard-watch is not installed beside hyprcustard or on your PATH."
                    wrapMode: Text.WordWrap
                    color: Theme.warn
                    font.pixelSize: Theme.fontSize
                }
                RowLayout {
                    width: parent.width
                    spacing: Theme.gap
                    Toggle {
                        text: "Keep following the wallpaper"
                        enabled: followCard.follow.installed === true
                                 && (page.mode === "wallpaper" || followCard.follow.on === true)
                        checked: followCard.follow.on === true
                        onToggled: App.run("colors.follow", { on: checked })
                    }
                    Badge {
                        visible: followCard.follow.on === true
                        text: !followCard.follow.running ? "Not running"
                              : followCard.status && followCard.status.state === "paused" ? "Paused"
                              : followCard.status && followCard.status.state === "error" ? "Problem"
                              : "Running"
                        tint: !followCard.follow.running || (followCard.status && followCard.status.state === "error")
                              ? Theme.warn : Theme.ok
                    }
                    Item {
                        Layout.fillWidth: true
                    }
                }
                Text {
                    visible: followCard.follow.on === true
                    width: parent.width
                    text: {
                        if (!followCard.follow.running)
                            return "It starts with your next login."
                        var st = followCard.status
                        if (!st)
                            return "Starting…"
                        var line = st.detail || ""
                        if (st.last_applied && st.state !== "error")
                            line += " · last changed to " + st.last_applied + " at " + st.at
                        return line
                    }
                    wrapMode: Text.WordWrap
                    color: followCard.status && followCard.status.state === "error" ? Theme.warn : Theme.fgDim
                    font.pixelSize: Theme.fontSizeSmall
                }
                LabeledSlider {
                    width: parent.width
                    label: "At most one change every (minutes)"
                    from: 0
                    to: 60
                    stepSize: 1
                    integer: true
                    value: page.choice.min_minutes === undefined ? 5 : page.choice.min_minutes
                    onSettled: function (v) { page.set({ min_minutes: Math.round(v) }) }
                }
                FieldRow {
                    visible: followCard.follow.systemd === true || page.choice.start_with === "systemd"
                    width: parent.width
                    label: "Start it"
                    hint: "With Hyprland adds a short block to your Hyprland config. Your session "
                          + "runs under systemd's graphical-session.target, so a user service works too."
                    Dropdown {
                        width: 300
                        model: ["With Hyprland", "As a systemd user service"]
                        currentIndex: page.choice.start_with === "systemd" ? 1 : 0
                        onActivated: function (index) {
                            page.set({ start_with: index === 1 ? "systemd" : "hyprland" })
                        }
                    }
                }
            }

            // -----------------------------------------------------------
            Card {
                Layout.fillWidth: true
                title: "Preview"
                subtitle: page.scheme ? page.scheme.name + (page.scheme.dark ? " · dark" : " · light")
                                      : page.building ? "Building…"
                                      : (page.built && !page.built.ok) ? "" : "Nothing to show yet"

                Text {
                    visible: !!page.built && !page.built.ok
                    width: parent.width
                    text: page.built && !page.built.ok ? page.built.error : ""
                    wrapMode: Text.WordWrap
                    color: Theme.warn
                    font.pixelSize: Theme.fontSize
                }

                // A small desktop wearing the scheme.
                Rectangle {
                    visible: !!page.scheme
                    width: parent.width
                    height: 210
                    radius: Theme.radiusSmall
                    color: page.role("surface_dim")
                    clip: true

                    // The bar.
                    Rectangle {
                        x: 10
                        y: 10
                        width: parent.width - 20
                        height: 28
                        radius: 6
                        color: page.role("surface_container")
                        opacity: page.choice.opacity || 1
                        Row {
                            x: 10
                            anchors.verticalCenter: parent.verticalCenter
                            spacing: 6
                            Repeater {
                                model: 5
                                Rectangle {
                                    required property int index
                                    width: 22
                                    height: 18
                                    radius: 4
                                    color: index === 0 ? page.role("primary") : page.role("surface_container_high")
                                    Text {
                                        anchors.centerIn: parent
                                        text: parent.index + 1
                                        color: parent.index === 0 ? page.role("on_primary") : page.role("on_surface_variant")
                                        font.pixelSize: Theme.fontSizeSmall
                                    }
                                }
                            }
                        }
                        Text {
                            anchors.centerIn: parent
                            text: "12:34"
                            color: page.role("on_surface")
                            font.pixelSize: Theme.fontSize
                        }
                    }

                    // A focused window: the border is the scheme's gradient.
                    Rectangle {
                        x: 10
                        y: 48
                        width: parent.width * 0.58
                        height: 150
                        radius: 8
                        gradient: Gradient {
                            orientation: Gradient.Horizontal
                            GradientStop { position: 0; color: page.role("primary") }
                            GradientStop { position: 1; color: page.role("tertiary") }
                        }
                        Rectangle {
                            anchors.fill: parent
                            anchors.margins: 2
                            radius: 6
                            color: page.role("surface")
                            Column {
                                x: 12
                                y: 10
                                spacing: 6
                                Text {
                                    text: "~ ❯ ls"
                                    color: page.role("on_surface")
                                    font.family: Theme.monoFamily
                                    font.pixelSize: Theme.fontSize
                                }
                                Row {
                                    spacing: 10
                                    Repeater {
                                        model: [
                                            { t: "src", b: 13 }, { t: "notes.md", b: 5 },
                                            { t: "build.sh", b: 11 }, { t: "todo", b: 10 }
                                        ]
                                        Text {
                                            required property var modelData
                                            text: modelData.t
                                            color: page.scheme ? page.scheme.base16[modelData.b] : "white"
                                            font.family: Theme.monoFamily
                                            font.pixelSize: Theme.fontSize
                                        }
                                    }
                                }
                                Text {
                                    text: "error: something went wrong"
                                    color: page.role("error")
                                    font.family: Theme.monoFamily
                                    font.pixelSize: Theme.fontSize
                                }
                                Row {
                                    spacing: 6
                                    Repeater {
                                        model: [
                                            { label: "Primary", bg: "primary", fg: "on_primary" },
                                            { label: "Secondary", bg: "secondary_container", fg: "on_secondary_container" },
                                            { label: "Tertiary", bg: "tertiary_container", fg: "on_tertiary_container" }
                                        ]
                                        Rectangle {
                                            required property var modelData
                                            width: chip.implicitWidth + 18
                                            height: 24
                                            radius: 12
                                            color: page.role(modelData.bg)
                                            Text {
                                                id: chip
                                                anchors.centerIn: parent
                                                text: parent.modelData.label
                                                color: page.role(parent.modelData.fg)
                                                font.pixelSize: Theme.fontSizeSmall
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // An unfocused window, and a notification.
                    Rectangle {
                        x: parent.width * 0.58 + 20
                        y: 48
                        width: parent.width * 0.42 - 30
                        height: 82
                        radius: 8
                        color: page.role("surface_container_low")
                        border.width: 2
                        border.color: page.role("outline_variant")
                        Text {
                            x: 12
                            y: 10
                            width: parent.width - 24
                            text: "An unfocused window"
                            elide: Text.ElideRight
                            color: page.role("on_surface_variant")
                            font.pixelSize: Theme.fontSize
                        }
                    }
                    Rectangle {
                        x: parent.width * 0.58 + 20
                        y: 140
                        width: parent.width * 0.42 - 30
                        height: 58
                        radius: 8
                        color: page.role("surface_container")
                        opacity: page.choice.opacity || 1
                        border.width: 2
                        border.color: page.role("primary")
                        Column {
                            x: 12
                            y: 8
                            width: parent.width - 24
                            Text {
                                width: parent.width
                                text: "Notification"
                                elide: Text.ElideRight
                                color: page.role("on_surface")
                                font.pixelSize: Theme.fontSize
                                font.bold: true
                            }
                            Text {
                                width: parent.width
                                text: "Your colors are ready"
                                elide: Text.ElideRight
                                color: page.role("on_surface_variant")
                                font.pixelSize: Theme.fontSizeSmall
                            }
                        }
                    }
                }

                // The names every colors file defines.
                Text {
                    visible: !!page.scheme
                    width: parent.width
                    text: "Every colors file defines these names, and the Material roles behind them "
                          + "(primary, on_surface, surface_container…). Hover a swatch for its name."
                    wrapMode: Text.WordWrap
                    color: Theme.muted
                    font.pixelSize: Theme.fontSizeSmall
                }
                Flow {
                    visible: !!page.scheme
                    width: parent.width
                    spacing: 4
                    Repeater {
                        model: ["surface", "surface_container", "surface_container_high", "outline_variant",
                                "outline", "on_surface_variant", "on_surface", "primary", "on_primary",
                                "primary_container", "secondary", "secondary_container", "tertiary",
                                "tertiary_container", "error", "error_container"]
                        ColorButton {
                            required property var modelData
                            implicitWidth: 40
                            value: page.role(modelData)
                            tooltip: modelData + "  " + page.role(modelData)
                        }
                    }
                }
                Flow {
                    visible: !!page.scheme
                    width: parent.width
                    spacing: 4
                    Repeater {
                        model: page.scheme ? page.scheme.base16 : []
                        ColorButton {
                            required property var modelData
                            required property int index
                            implicitWidth: 40
                            value: modelData
                            tooltip: "base0" + index.toString(16).toUpperCase() + "  " + modelData
                        }
                    }
                }
            }

            // -----------------------------------------------------------
            Card {
                Layout.fillWidth: true
                title: "Apps"
                subtitle: "Each app turned on gets its own colors file next to its config. "
                          + "Its config has to load that file once — the line to add is shown, "
                          + "and the app can add it for you (backing the config up first)."

                Repeater {
                    model: page.installedTargets

                    Rectangle {
                        id: row
                        required property var modelData
                        readonly property var t: row.modelData
                        // Handing the app's own colors over: the list is open,
                        // and the pairings changed from the guesses, by name.
                        property bool pairing: false
                        property var picks: ({})
                        width: parent ? parent.width : 0
                        height: rowColumn.implicitHeight + 2 * Theme.gap
                        radius: Theme.radiusSmall
                        color: Theme.surface
                        border.width: 1
                        border.color: Theme.border

                        Column {
                            id: rowColumn
                            x: Theme.gap
                            y: Theme.gap
                            width: parent.width - 2 * Theme.gap
                            spacing: Theme.gapSmall

                            RowLayout {
                                width: parent.width
                                spacing: Theme.gap
                                Toggle {
                                    id: targetSwitch
                                    text: row.t.name
                                    checked: row.t.on
                                    onToggled: page.setTarget(row.t.id, checked)

                                    Tip {
                                        visible: targetSwitch.hovered
                                        delay: 400
                                        text: "On: applying a scheme writes " + row.t.name + "'s colors file. "
                                              + "Whether " + row.t.name + " uses it is the badge beside this, "
                                              + "which follows its own config."
                                    }
                                }
                                Badge {
                                    visible: row.t.on
                                    text: row.t.wired ? "Loads the colors"
                                          : row.t.id === "dunst" ? "Needs its own dunstrc"
                                          : row.t.replaces ? "Its gtk.css is a theme"
                                          : row.t.id === "kvantum" ? "Not the Kvantum theme"
                                          : (row.t.id === "qt6ct" || row.t.id === "qt5ct") ? "Needs a setting" : "Needs one line"
                                    tint: row.t.wired ? Theme.ok : Theme.warn
                                }
                                Item {
                                    Layout.fillWidth: true
                                }
                                FitText {
                                    Layout.maximumWidth: 360
                                    text: row.t.file
                                    color: Theme.muted
                                    font.pixelSize: Theme.fontSizeSmall
                                }
                            }
                            Text {
                                visible: row.t.on
                                width: parent.width
                                text: row.t.usage
                                wrapMode: Text.WordWrap
                                color: Theme.fgDim
                                font.pixelSize: Theme.fontSizeSmall
                            }
                            Text {
                                visible: row.t.on && row.t.id === "hyprland"
                                         && (page.colors.hyprlandConflicts || []).length > 0
                                width: parent.width
                                text: "The Look page also sets " + (page.colors.hyprlandConflicts || []).join(", ")
                                      .toLowerCase() + ". Whichever comes later in your config wins — "
                                      + "reset them on the Look page to let the scheme decide."
                                wrapMode: Text.WordWrap
                                color: Theme.warn
                                font.pixelSize: Theme.fontSizeSmall
                            }
                            RowLayout {
                                visible: row.t.on && (row.t.warning || "") !== ""
                                width: parent.width
                                spacing: Theme.gap
                                Text {
                                    Layout.fillWidth: true
                                    text: row.t.warning || ""
                                    wrapMode: Text.WordWrap
                                    color: Theme.warn
                                    font.pixelSize: Theme.fontSizeSmall
                                }
                                PillButton {
                                    visible: (row.t.fix || "") !== ""
                                    text: row.t.fix || ""
                                    tooltip: "Sets the GTK theme in gsettings and in your settings.ini files, "
                                             + "backing those up first"
                                    onClicked: App.run("colors.fix", { id: row.t.id })
                                }
                            }
                            // The app's own colors, defined in its config after
                            // the line that loads the scheme's, so they win —
                            // offered to the scheme name by name.
                            RowLayout {
                                visible: row.t.on && (row.t.own || []).length > 0
                                width: parent.width
                                spacing: Theme.gap
                                Text {
                                    Layout.fillWidth: true
                                    text: row.t.config + " sets " + App.plural((row.t.own || []).length, "color")
                                          + " of its own, so the scheme cannot change them: "
                                          + (row.t.own || []).map(function (o) { return o.name }).join(", ") + "."
                                    wrapMode: Text.WordWrap
                                    color: Theme.warn
                                    font.pixelSize: Theme.fontSizeSmall
                                }
                                PillButton {
                                    text: row.pairing ? "Not now" : "Let the scheme set them…"
                                    onClicked: {
                                        row.picks = ({})
                                        row.pairing = !row.pairing
                                    }
                                }
                            }
                            Column {
                                visible: row.pairing && row.t.on && (row.t.own || []).length > 0
                                width: parent.width
                                spacing: Theme.gapSmall

                                Text {
                                    width: parent.width
                                    text: "Each of your colors takes the scheme color beside it, keeping its own "
                                          + "transparency. Your lines in " + row.t.config + " are commented out with "
                                          + "a marker naming that color, so Give them back restores them exactly. "
                                          + "A backup is made first."
                                    wrapMode: Text.WordWrap
                                    color: Theme.fgDim
                                    font.pixelSize: Theme.fontSizeSmall
                                }

                                Repeater {
                                    model: row.t.own || []

                                    RowLayout {
                                        id: pair
                                        required property var modelData
                                        readonly property string role: row.picks[pair.modelData.name] !== undefined
                                                                       ? row.picks[pair.modelData.name]
                                                                       : pair.modelData.role
                                        width: parent ? parent.width : 0
                                        spacing: Theme.gap

                                        Rectangle {
                                            implicitWidth: 18
                                            implicitHeight: 18
                                            radius: 3
                                            color: pair.modelData.swatch
                                            border.width: 1
                                            border.color: Theme.border
                                        }
                                        Text {
                                            Layout.preferredWidth: 220
                                            text: pair.modelData.name
                                            color: Theme.fg
                                            font.family: Theme.monoFamily
                                            font.pixelSize: Theme.fontSizeSmall
                                            elide: Text.ElideRight
                                        }
                                        Text {
                                            text: "→"
                                            color: Theme.muted
                                            font.pixelSize: Theme.fontSize
                                        }
                                        Dropdown {
                                            Layout.preferredWidth: 240
                                            model: ["keep mine"].concat((page.colors.colorChoices || []).map(page.choiceLabel))
                                            currentIndex: pair.role === "" ? 0
                                                          : (page.colors.colorChoices || []).indexOf(pair.role) + 1
                                            onActivated: function (index) {
                                                var picks = Object.assign({}, row.picks)
                                                picks[pair.modelData.name] = index === 0 ? "" : page.colors.colorChoices[index - 1]
                                                row.picks = picks
                                            }
                                        }
                                        Rectangle {
                                            visible: pair.role !== "" && !!page.colors.choiceColors
                                            implicitWidth: 18
                                            implicitHeight: 18
                                            radius: 3
                                            color: page.colors.choiceColors ? (page.colors.choiceColors[pair.role] || "transparent")
                                                                            : "transparent"
                                            border.width: 1
                                            border.color: Theme.border
                                        }
                                        Item {
                                            Layout.fillWidth: true
                                        }
                                    }
                                }

                                PillButton {
                                    text: "Hand them over"
                                    onClicked: {
                                        var pairs = []
                                        var own = row.t.own || []
                                        for (var i = 0; i < own.length; i++) {
                                            var name = own[i].name
                                            var role = row.picks[name] !== undefined ? row.picks[name] : own[i].role
                                            if (role !== "")
                                                pairs.push({ name: name, role: role })
                                        }
                                        App.run("colors.handOver", { id: row.t.id, pairs: pairs })
                                        row.pairing = false
                                    }
                                }
                            }
                            RowLayout {
                                visible: row.t.on && (row.t.taken || []).length > 0
                                width: parent.width
                                spacing: Theme.gap
                                Text {
                                    Layout.fillWidth: true
                                    text: App.plural((row.t.taken || []).length, "color") + " from " + row.t.config
                                          + " now come from the scheme: "
                                          + (row.t.taken || []).map(function (o) {
                                                return o.name + " → " + page.choiceLabel(o.role)
                                            }).join(", ") + "."
                                    wrapMode: Text.WordWrap
                                    color: Theme.fgDim
                                    font.pixelSize: Theme.fontSizeSmall
                                }
                                PillButton {
                                    text: "Give them back"
                                    tooltip: "Uncomment your own lines in " + row.t.config + ", exactly as they were"
                                    onClicked: App.run("colors.giveBack", { id: row.t.id })
                                }
                            }
                            Text {
                                visible: row.t.on && !row.t.wired && row.t.line !== ""
                                width: parent.width
                                text: row.t.replaces
                                      ? "A new " + row.t.config + " would hold just this line:"
                                      : "Add this line " + row.t.place + " " + row.t.config + ":"
                                wrapMode: Text.WordWrap
                                color: Theme.fgDim
                                font.pixelSize: Theme.fontSizeSmall
                            }
                            RowLayout {
                                visible: row.t.on && !row.t.wired
                                width: parent.width
                                spacing: Theme.gap
                                LineEdit {
                                    visible: row.t.line !== ""
                                    Layout.fillWidth: true
                                    mono: true
                                    readOnly: true
                                    selectByMouse: true
                                    text: row.t.line
                                }
                                Text {
                                    visible: row.t.line === ""
                                    Layout.fillWidth: true
                                    text: "Dunst reads extra settings only beside your own "
                                          + row.t.config + ", and you are using the system one."
                                    wrapMode: Text.WordWrap
                                    color: Theme.fgDim
                                    font.pixelSize: Theme.fontSizeSmall
                                }
                                PillButton {
                                    text: row.t.line === "" ? "Make my own dunstrc"
                                          : row.t.replaces ? "Set it aside for me" : "Add it for me"
                                    tooltip: row.t.line === ""
                                             ? "Copies the system dunstrc to " + row.t.config
                                             : row.t.replaces
                                               ? "Backs up " + row.t.config + " and starts a new one "
                                                 + "that loads the colors"
                                               : "Adds the line " + row.t.place + " " + row.t.config
                                                 + ", after backing it up"
                                    onClicked: App.run("colors.wire", { id: row.t.id })
                                }
                            }
                        }
                    }
                }

                Text {
                    visible: page.missingTargets.length > 0
                    width: parent.width
                    text: "Not installed: " + page.missingTargets.map(function (t) { return t.name }).join(", ") + "."
                    wrapMode: Text.WordWrap
                    color: Theme.muted
                    font.pixelSize: Theme.fontSizeSmall
                }

                LabeledSlider {
                    width: parent.width
                    label: "Bar and popup opacity"
                    from: 0.3
                    to: 1
                    stepSize: 0.05
                    value: page.choice.opacity || 1
                    onSettled: function (v) { page.set({ opacity: Math.round(v * 100) / 100 }) }
                }
                Text {
                    width: parent.width
                    text: "Used for notification backgrounds, and for the _alpha names "
                          + "(surface_alpha and friends) in the bar and launcher files."
                    wrapMode: Text.WordWrap
                    color: Theme.muted
                    font.pixelSize: Theme.fontSizeSmall
                }

                Toggle {
                    text: "Use the applied scheme for hyprcustard's own window too"
                    checked: page.colors.appUsesScheme === true
                    onToggled: App.run("config.update", { theme: checked ? "scheme" : "system" })
                }
            }

            Item {
                Layout.preferredHeight: Theme.pad
            }
        }
    }
}
