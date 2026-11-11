import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import dev.hyprcustard.gui

// A themed "are you sure".
//
// Reserved for the two things that cannot simply be undone by pressing the
// other button: rewriting hyprland.lua, and throwing away unsaved edits.
// Everything else in the app is reversible and is not worth a dialog.
Item {
    id: root

    anchors.fill: parent
    visible: false
    z: 200

    property string heading: ""
    property string body: ""
    property string acceptText: "Continue"
    property bool danger: false
    property var _onAccept: null
    // An optional second way forward, between Cancel and the main button —
    // for a question with two real answers, such as replace or keep both.
    property string altText: ""
    property var _onAlt: null

    // `onAccept` is a function called if the user confirms. Nothing happens on
    // cancel, which is the point.
    function ask(heading, body, acceptText, danger, onAccept) {
        root.choose(heading, body, acceptText, danger, onAccept, "", null)
    }

    // `ask`, with a second answer: `onAlt` runs if the user picks `altText`.
    function choose(heading, body, acceptText, danger, onAccept, altText, onAlt) {
        root.heading = heading
        root.body = body
        root.acceptText = acceptText
        root.danger = danger === true
        root._onAccept = onAccept
        root.altText = altText || ""
        root._onAlt = onAlt || null
        root.visible = true
        acceptButton.forceActiveFocus()
    }

    function alt() {
        var fn = root._onAlt
        root.close()
        if (fn)
            fn()
    }

    function accept() {
        var fn = root._onAccept
        root.close()
        if (fn)
            fn()
    }

    function close() {
        root.visible = false
        root._onAccept = null
        root._onAlt = null
    }

    // Swallows clicks and keystrokes aimed at the interface behind it.
    MouseArea {
        anchors.fill: parent
        hoverEnabled: true
        onClicked: root.close()
    }

    Rectangle {
        anchors.fill: parent
        color: Theme.bg
        opacity: 0.7
    }

    Rectangle {
        id: panel

        anchors.centerIn: parent
        width: Math.min(460, root.width - 2 * Theme.pad)
        height: content.implicitHeight + 2 * Theme.pad
        radius: Theme.radius
        color: Theme.surface
        border.width: 1
        border.color: root.danger ? Theme.error : Theme.border

        // Clicks on the panel itself must not dismiss it.
        MouseArea {
            anchors.fill: parent
        }

        ColumnLayout {
            id: content
            anchors.fill: parent
            anchors.margins: Theme.pad
            spacing: Theme.gap

            Text {
                Layout.fillWidth: true
                text: root.heading
                color: Theme.fg
                font.pixelSize: Theme.fontSizeTitle
                font.weight: Font.DemiBold
                wrapMode: Text.WordWrap
            }

            Text {
                Layout.fillWidth: true
                text: root.body
                color: Theme.fgDim
                font.pixelSize: Theme.fontSize
                wrapMode: Text.WordWrap
            }

            RowLayout {
                Layout.fillWidth: true
                Layout.topMargin: Theme.gapSmall
                spacing: Theme.gapSmall

                Item {
                    Layout.fillWidth: true
                }

                PillButton {
                    text: "Cancel"
                    onClicked: root.close()
                }

                PillButton {
                    visible: root.altText !== ""
                    text: root.altText
                    onClicked: root.alt()
                }

                PillButton {
                    id: acceptButton
                    text: root.acceptText
                    danger: root.danger
                    primary: !root.danger
                    onClicked: root.accept()
                }
            }
        }
    }

    Keys.onEscapePressed: root.close()
    Keys.onReturnPressed: root.accept()
    Keys.onEnterPressed: root.accept()

    focus: visible
}
