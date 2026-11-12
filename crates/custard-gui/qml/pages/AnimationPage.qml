import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import dev.hyprcustard.gui

// Hyprland's animations: every leaf of the tree, the curve each runs on, and
// the curves themselves.
//
// A leaf nothing sets inherits from its parent, the way Hyprland resolves
// them, so the list says where each one's values come from. Changing a leaf
// sets it here — all of its values at once, as `hl.animation` takes them —
// and the change goes into the look block on Save.
Item {
    id: page

    readonly property var motion: App.state.motion || ({ leaves: [], curves: [] })
    readonly property bool running: App.look.running === true
    readonly property bool liveEdits: App.settings.live_look_edits !== false

    property string selected: "windows"

    readonly property var byName: {
        var m = ({})
        var ls = page.motion.leaves || []
        for (var i = 0; i < ls.length; ++i)
            m[ls[i].name] = ls[i]
        return m
    }
    readonly property var curveByName: {
        var m = ({})
        var cs = page.motion.curves || []
        for (var i = 0; i < cs.length; ++i)
            m[cs[i].name] = cs[i]
        return m
    }
    readonly property var curveNames: {
        var out = []
        var cs = page.motion.curves || []
        for (var i = 0; i < cs.length; ++i)
            out.push(cs[i].name)
        return out
    }

    // The list's model: names only, built once, so a state push updates the
    // rows in place instead of resetting the list and its scroll position.
    property var leafNames: []
    function buildNames() {
        var ls = page.motion.leaves || []
        if (page.leafNames.length > 0 || ls.length === 0)
            return
        var out = []
        for (var i = 0; i < ls.length; ++i)
            out.push(ls[i].name)
        page.leafNames = out
    }
    Component.onCompleted: page.buildNames()
    Connections {
        target: App
        function onStateChanged() {
            page.buildNames()
        }
    }

    readonly property var leaf: page.byName[page.selected] || null
    readonly property var setting: page.leaf ? page.leaf.setting
                                             : ({ enabled: true, speed: 1, bezier: "default", style: "" })
    readonly property var curve: page.curveByName[page.setting.bezier] || null
    // Null when the curve is known only by name: written in a way the file
    // reader could not follow, with Hyprland not running to ask.
    readonly property bool curveKnown: page.curve !== null && page.curve.points !== null
    readonly property var curvePoints: page.curveKnown ? page.curve.points : [0, 0, 1, 1]

    // Every change sets the whole leaf: hl.animation takes all of it at once.
    function setLeaf(changes) {
        var s = JSON.parse(JSON.stringify(page.setting))
        for (var k in changes)
            s[k] = changes[k]
        App.run("motion.set", { leaf: page.selected, setting: s })
    }

    function styleBase(style) {
        return String(style || "").split(" ")[0] || ""
    }
    function styleParam(style) {
        return String(style || "").split(" ")[1] || ""
    }
    function paramKind(base) {
        var styles = page.leaf ? page.leaf.styles : []
        for (var i = 0; i < styles.length; ++i)
            if (styles[i].name === base)
                return styles[i].param
        // A style the page does not list, already in someone's config.
        if (/^(popin|slidefade)/.test(base))
            return "percent"
        return "none"
    }
    function sourceText(l) {
        if (!l)
            return ""
        switch (l.source) {
        case "here":
            return l.changed ? "Set here — not saved yet" : "Set here"
        case "config":
            return "Set in your config"
        case "inherited":
            return "Inherits from " + l.from
        }
        return "Hyprland's default"
    }
    function summary(s) {
        if (!s.enabled)
            return "off"
        return (Math.round(s.speed * 100)) + " ms · " + s.bezier
    }

    RowLayout {
        anchors.fill: parent
        spacing: Theme.pad

        // ------------------------------------------------------------------
        // The tree
        // ------------------------------------------------------------------
        Rectangle {
            Layout.preferredWidth: Theme.listWidth
            Layout.minimumWidth: Theme.listWidth
            Layout.maximumWidth: Theme.listWidth
            Layout.fillHeight: true
            radius: Theme.radius
            color: Theme.surface
            border.width: 1
            border.color: Theme.border

            ListView {
                id: list
                anchors.fill: parent
                anchors.margins: 4
                clip: true
                spacing: 1
                model: page.leafNames
                boundsBehavior: Flickable.StopAtBounds
                ScrollBar.vertical: ScrollBar {}

                delegate: Rectangle {
                    id: leafRow

                    required property var modelData
                    readonly property var l: page.byName[leafRow.modelData] || ({ setting: {} })
                    readonly property bool current: page.selected === leafRow.modelData

                    width: list.width - 6
                    height: 40
                    radius: Theme.radiusSmall
                    color: leafRow.current ? Theme.selection
                                           : (rowHover.hovered ? Theme.surfaceHi : "transparent")

                    HoverHandler {
                        id: rowHover
                    }
                    TapHandler {
                        onTapped: page.selected = leafRow.modelData
                    }

                    // Set here: accent once saved, warning color while not.
                    Rectangle {
                        visible: leafRow.l.overridden === true
                        width: 3
                        height: 24
                        radius: 1.5
                        anchors.verticalCenter: parent.verticalCenter
                        color: leafRow.l.changed ? Theme.warn : Theme.accent
                    }

                    Column {
                        anchors.verticalCenter: parent.verticalCenter
                        anchors.left: parent.left
                        anchors.leftMargin: 10 + (leafRow.l.depth || 0) * 14
                        anchors.right: parent.right
                        anchors.rightMargin: Theme.gapSmall
                        spacing: 1

                        Row {
                            width: parent.width
                            spacing: Theme.gapSmall

                            Text {
                                text: leafRow.modelData
                                font.family: Theme.monoFamily
                                font.pixelSize: Theme.fontSize
                                font.strikeout: leafRow.l.setting.enabled === false
                                color: leafRow.l.source === "inherited" || leafRow.l.source === "default"
                                       ? Theme.fgDim : Theme.fg
                            }
                        }
                        FitText {
                            width: parent.width
                            elide: Text.ElideRight
                            text: leafRow.l.source === "inherited"
                                  ? "from " + leafRow.l.from + " · " + page.summary(leafRow.l.setting)
                                  : page.summary(leafRow.l.setting)
                            color: Theme.muted
                            font.pixelSize: Theme.fontSizeSmall
                        }
                    }
                }
            }
        }

        // ------------------------------------------------------------------
        // The selected leaf
        // ------------------------------------------------------------------
        ScrollView {
            id: scroll
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            contentWidth: availableWidth

            ColumnLayout {
                width: Math.min(scroll.availableWidth, Theme.contentMax)
                spacing: Theme.pad

                Card {
                    Layout.fillWidth: true
                    title: page.selected
                    subtitle: page.leaf ? page.leaf.label : ""

                    RowLayout {
                        width: parent.width
                        spacing: Theme.gap

                        Badge {
                            text: page.sourceText(page.leaf)
                            tint: page.leaf && page.leaf.source === "here"
                                  ? (page.leaf.changed ? Theme.warn : Theme.accent) : Theme.muted
                        }
                        Item {
                            Layout.fillWidth: true
                        }
                        PillButton {
                            compact: true
                            text: "Reset"
                            enabled: page.leaf !== null && page.leaf.overridden === true
                            tooltip: "Stop setting this here, so it goes back to your config or "
                                     + "its parent once saved."
                            onClicked: App.run("motion.reset", { leaf: page.selected })
                        }
                    }

                    Text {
                        width: parent.width
                        visible: page.leaf !== null && page.leaf.source === "inherited"
                        wrapMode: Text.WordWrap
                        color: Theme.muted
                        font.pixelSize: Theme.fontSizeSmall
                        text: "Changing anything here sets " + page.selected
                              + " on its own, so it stops following " + (page.leaf ? page.leaf.from : "")
                              + ". Leave it alone to set " + (page.leaf ? page.leaf.from : "")
                              + " instead and change every animation under it."
                    }

                    Toggle {
                        id: animateToggle
                        text: "Animate"
                        checked: page.setting.enabled === true
                        onToggled: {
                            var v = animateToggle.checked
                            animateToggle.checked = Qt.binding(function () {
                                return page.setting.enabled === true
                            })
                            page.setLeaf({ enabled: v })
                        }
                    }

                    LabeledSlider {
                        id: speedSlider
                        width: parent.width
                        enabled: page.setting.enabled === true
                        label: "Speed — " + Math.round(speedSlider.value * 100) + " ms"
                        from: 0.1
                        to: 20
                        stepSize: 0.1
                        Component.onCompleted: speedSlider.value = page.setting.speed
                        Connections {
                            target: page
                            function onSettingChanged() {
                                if (!speedSlider.pressed)
                                    speedSlider.value = page.setting.speed
                            }
                        }
                        onSettled: function (v) {
                            if (Math.abs(v - page.setting.speed) > 0.001)
                                page.setLeaf({ speed: v })
                        }
                    }

                    FieldRow {
                        width: parent.width
                        label: "Curve"
                        labelWidth: 90
                        enabled: page.setting.enabled === true

                        Dropdown {
                            id: curvePick
                            width: Math.min(parent.width, 280)
                            model: page.curveNames
                            currentIndex: page.curveNames.indexOf(page.setting.bezier)
                            onActivated: function (index) {
                                var name = page.curveNames[index]
                                curvePick.currentIndex = Qt.binding(function () {
                                    return page.curveNames.indexOf(page.setting.bezier)
                                })
                                if (name !== page.setting.bezier)
                                    page.setLeaf({ bezier: name })
                            }
                        }
                    }

                    FieldRow {
                        width: parent.width
                        label: "Style"
                        labelWidth: 90
                        visible: page.leaf !== null
                                 && (page.leaf.styles.length > 0 || page.setting.style !== "")
                        enabled: page.setting.enabled === true

                        Row {
                            spacing: Theme.gap

                            Dropdown {
                                id: stylePick

                                // The family's styles, plus whatever the leaf
                                // already uses: the page's lists are not all
                                // Hyprland accepts, and a style in use must
                                // show as itself rather than as "default".
                                readonly property var names: {
                                    var out = ["default"]
                                    var styles = page.leaf ? page.leaf.styles : []
                                    for (var i = 0; i < styles.length; ++i)
                                        out.push(styles[i].name)
                                    var current = page.styleBase(page.setting.style)
                                    if (current !== "" && out.indexOf(current) < 0)
                                        out.push(current)
                                    return out
                                }

                                width: 170
                                model: stylePick.names
                                currentIndex: Math.max(0, stylePick.names.indexOf(
                                                           page.styleBase(page.setting.style)))
                                onActivated: function (index) {
                                    var base = index === 0 ? "" : stylePick.names[index]
                                    stylePick.currentIndex = Qt.binding(function () {
                                        return Math.max(0, stylePick.names.indexOf(
                                                            page.styleBase(page.setting.style)))
                                    })
                                    var oldBase = page.styleBase(page.setting.style)
                                    var keep = page.paramKind(base) === page.paramKind(oldBase)
                                    var param = keep ? page.styleParam(page.setting.style) : ""
                                    var style = base === "" ? "" : (param === "" ? base : base + " " + param)
                                    if (style !== page.setting.style)
                                        page.setLeaf({ style: style })
                                }
                            }

                            Dropdown {
                                id: sidePick

                                readonly property var sides: ["auto", "top", "bottom", "left", "right"]

                                visible: page.paramKind(page.styleBase(page.setting.style)) === "side"
                                width: 120
                                model: sidePick.sides
                                currentIndex: Math.max(0, sidePick.sides.indexOf(
                                                           page.styleParam(page.setting.style)))
                                onActivated: function (index) {
                                    sidePick.currentIndex = Qt.binding(function () {
                                        return Math.max(0, sidePick.sides.indexOf(
                                                            page.styleParam(page.setting.style)))
                                    })
                                    var base = page.styleBase(page.setting.style)
                                    page.setLeaf({ style: index === 0 ? base : base + " " + sidePick.sides[index] })
                                }
                            }

                            LineEdit {
                                id: percentEdit
                                visible: page.paramKind(page.styleBase(page.setting.style)) === "percent"
                                width: 90
                                mono: true
                                placeholderText: "80%"
                                text: page.styleParam(page.setting.style)
                                onCommit: function (v) {
                                    percentEdit.text = Qt.binding(function () {
                                        return page.styleParam(page.setting.style)
                                    })
                                    var n = parseFloat(v)
                                    var base = page.styleBase(page.setting.style)
                                    var style = isNaN(n) ? base : base + " " + Math.round(n) + "%"
                                    if (style !== page.setting.style)
                                        page.setLeaf({ style: style })
                                }
                            }
                        }
                    }
                }

                Card {
                    Layout.fillWidth: true
                    title: "Preview"
                    subtitle: "An approximation, on your curve and at your speed. With live "
                              + "edits on, the real thing is already on your desktop — open a "
                              + "window or switch workspace to see it."

                    AnimationDemo {
                        id: demo
                        width: parent.width
                        family: page.leaf ? page.leaf.family : "plain"
                        styleText: page.setting.style || ""
                        speed: page.setting.speed
                        points: curveEdit.shown
                        animate: page.setting.enabled === true
                    }

                    Row {
                        spacing: Theme.gap
                        PillButton {
                            text: "Play"
                            onClicked: demo.play()
                        }
                        Toggle {
                            text: "Repeat"
                            checked: demo.repeat
                            onToggled: demo.repeat = checked
                        }
                    }
                }

                Card {
                    Layout.fillWidth: true
                    title: "Curve — " + page.setting.bezier
                    subtitle: {
                        if (!page.curve)
                            return "Hyprland is not running, and hyprland.lua does not define "
                                   + "this curve, so it cannot be shown."
                        if (!page.curveKnown)
                            return "Defined in your config in a way the app cannot read the "
                                   + "points from. Start Hyprland to see and edit it."
                        switch (page.curve.source) {
                        case "here":
                            return "Defined here."
                        case "builtin":
                            return "Hyprland's built-in curve. Dragging a point redefines it here, "
                                   + "for every animation that uses it."
                        case "config":
                            return "Defined in your config. Dragging a point redefines it here, "
                                   + "for every animation that uses it."
                        }
                        return "Not defined in hyprland.lua. Hyprland still has it: from a file "
                               + "your config loads, or left over from an earlier edit, since "
                               + "Hyprland keeps a curve until it restarts. If nothing defines it, "
                               + "an animation using it will fail to load next time."
                    }

                    CurveEditor {
                        id: curveEdit
                        width: parent.width
                        height: 260
                        points: page.curvePoints
                        editable: page.curveKnown
                        onCommitted: function (p) {
                            App.run("curve.set", { name: page.setting.bezier, points: p })
                        }
                    }

                    Row {
                        spacing: Theme.gapSmall

                        Repeater {
                            model: ["x0", "y0", "x1", "y1"]

                            Row {
                                id: coord
                                required property var modelData
                                required property int index
                                spacing: 4

                                Text {
                                    anchors.verticalCenter: parent.verticalCenter
                                    text: coord.modelData
                                    color: Theme.muted
                                    font.pixelSize: Theme.fontSizeSmall
                                }
                                LineEdit {
                                    id: coordEdit
                                    width: 70
                                    implicitHeight: 26
                                    mono: true
                                    enabled: page.curveKnown
                                    text: String(curveEdit.shown[coord.index])
                                    onCommit: function (v) {
                                        coordEdit.text = Qt.binding(function () {
                                            return String(curveEdit.shown[coord.index])
                                        })
                                        var n = parseFloat(v)
                                        if (isNaN(n) || n === page.curvePoints[coord.index])
                                            return
                                        var p = page.curvePoints.slice()
                                        p[coord.index] = n
                                        App.run("curve.set", { name: page.setting.bezier, points: p })
                                    }
                                }
                            }
                        }
                    }

                    RowLayout {
                        width: parent.width
                        spacing: Theme.gap

                        LineEdit {
                            id: newName
                            Layout.preferredWidth: 200
                            mono: true
                            placeholderText: "new-curve-name"
                        }
                        PillButton {
                            text: "Copy as new curve"
                            enabled: newName.text.trim() !== "" && page.curveKnown
                            tooltip: "Make a copy of this curve under a new name and use it for "
                                     + page.selected + ", leaving the original alone."
                            onClicked: {
                                var name = newName.text.trim()
                                App.run("curve.set", { name: name, points: page.curvePoints })
                                if (page.curveByName[name])
                                    page.setLeaf({ bezier: name })
                                newName.text = ""
                            }
                        }
                        Item {
                            Layout.fillWidth: true
                        }
                        PillButton {
                            text: "Remove"
                            danger: true
                            visible: page.curve !== null && page.curve.source === "here"
                            tooltip: "Stop defining this curve here."
                            onClicked: App.run("curve.remove", { name: page.setting.bezier })
                        }
                    }
                }

                Card {
                    Layout.fillWidth: true

                    Text {
                        width: parent.width
                        wrapMode: Text.WordWrap
                        color: page.running ? Theme.fgDim : Theme.warn
                        font.pixelSize: Theme.fontSizeSmall
                        text: {
                            if (!page.running)
                                return "Hyprland is not running, so only what is set here is "
                                       + "shown. Changes are still saved."
                            var n = page.motion.count || 0
                            return (n === 0 ? "Nothing about animation is set here yet."
                                            : App.plural(n, "animation or curve") + " set here.")
                                   + (page.liveEdits ? " Changes go onto the desktop as you make them;"
                                                       + " Save writes them to hyprland.lua."
                                                     : " Live edits are off (see the Look page).")
                        }
                    }
                    PillButton {
                        text: "Clear all animation settings"
                        danger: true
                        enabled: (page.motion.count || 0) > 0
                        onClicked: App.confirm(
                            "Clear every animation and curve set here?",
                            "Your own config's animations take over again once you save.",
                            "Clear", true,
                            function () { App.run("motion.resetAll", {}) })
                    }
                }

                Item {
                    Layout.preferredHeight: Theme.pad
                }
            }
        }
    }
}
