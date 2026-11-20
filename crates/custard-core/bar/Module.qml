import QtQuick
import QtQuick.Layouts

// One bar section: padded text, optional colored edge, hover tooltip and
// mouse handling. `minText` reserves width so changing numbers (CPU, memory,
// volume...) never shift the modules next to it.
Item {
    id: m

    // Turned off on hyprcustard's Bar page: hidden, and its polling stopped.
    property bool shown: true
    property string text: ""
    property string minText: ""
    property real fontSize: Theme.fontSize
    property real padLeft: 11
    property real padRight: 11
    property real padTop: 0
    property int edgeLeft: 0
    property int edgeRight: 0
    // The colored edges are the flat style's; a neumorphic bar has none.
    readonly property int _edgeLeft: Config.neumorphic ? 0 : edgeLeft
    readonly property int _edgeRight: Config.neumorphic ? 0 : edgeRight
    property color edgeColor: "transparent"
    property real maxTextWidth: 0
    property bool rich: false
    property string tooltip: ""
    property bool tooltipRich: false
    property int buttons: Qt.NoButton
    readonly property alias hovered: hover.hovered

    signal clicked(var mouse)
    signal doubleClicked(var mouse)
    signal scrolled(var wheel)

    visible: shown
    Layout.fillHeight: true
    implicitWidth: _edgeLeft + padLeft + labelBox.width + padRight + _edgeRight

    TextMetrics {
        id: reserve
        font: label.font
        text: m.minText
    }

    Rectangle {
        visible: m._edgeLeft > 0
        anchors { left: parent.left; top: parent.top; bottom: parent.bottom }
        width: m._edgeLeft
        color: m.edgeColor
    }
    Rectangle {
        visible: m._edgeRight > 0
        anchors { right: parent.right; top: parent.top; bottom: parent.bottom }
        width: m._edgeRight
        color: m.edgeColor
    }

    Item {
        id: labelBox
        x: m._edgeLeft + m.padLeft
        anchors.verticalCenter: parent.verticalCenter
        anchors.verticalCenterOffset: m.padTop / 2
        width: {
            const w = Math.max(label.implicitWidth, m.minText === "" ? 0 : Math.ceil(reserve.advanceWidth));
            return m.maxTextWidth > 0 ? Math.min(w, m.maxTextWidth) : w;
        }
        height: label.implicitHeight
        Text {
            id: label
            anchors.centerIn: parent
            width: Math.min(implicitWidth, parent.width)
            text: m.text
            elide: Text.ElideRight
            textFormat: m.rich ? Text.RichText : Text.PlainText
            color: Theme.foreground
            font.family: Theme.font
            font.pixelSize: m.fontSize
        }
    }

    HoverHandler { id: hover }

    MouseArea {
        anchors.fill: parent
        acceptedButtons: m.buttons
        enabled: m.buttons !== Qt.NoButton
        cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
        onClicked: mouse => m.clicked(mouse)
        onDoubleClicked: mouse => m.doubleClicked(mouse)
        onWheel: wheel => m.scrolled(wheel)
    }

    Tooltip {
        target: m
        hovered: m.hovered
        text: m.tooltip
        rich: m.tooltipRich
    }
}
