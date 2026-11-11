import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import dev.hyprcustard.gui

// sudo's password prompt, in a window.
//
// hyprpm escalates by itself for the few steps that write outside $HOME, and
// with no terminal to read from, sudo asks this app instead. What is typed
// here goes straight back to sudo over a socket: it is never written down, and
// it is forgotten as soon as the operation ends.
Item {
    id: root

    anchors.fill: parent
    visible: false
    z: 300

    // What sudo asked, verbatim — it names the user the password is for.
    property string prompt: ""
    // True when the last password was rejected.
    property bool retry: false

    signal answered(string password)
    signal refused

    function ask(prompt, retry) {
        root.prompt = prompt
        root.retry = retry === true
        field.text = ""
        root.visible = true
        field.forceActiveFocus()
    }

    function submit() {
        var value = field.text
        field.text = ""
        root.visible = false
        root.answered(value)
    }

    // Saying no has to reach sudo, or it sits there waiting for an answer that
    // is never coming.
    function refuse() {
        field.text = ""
        root.visible = false
        root.refused()
    }

    // Swallows everything aimed at the interface behind it. Unlike the confirm
    // dialog, a click outside does not dismiss it: sudo is waiting.
    MouseArea {
        anchors.fill: parent
        hoverEnabled: true
    }

    Rectangle {
        anchors.fill: parent
        color: Theme.bg
        opacity: 0.7
    }

    Rectangle {
        anchors.centerIn: parent
        width: Math.min(460, root.width - 2 * Theme.pad)
        height: content.implicitHeight + 2 * Theme.pad
        radius: Theme.radius
        color: Theme.surface
        border.width: 1
        border.color: root.retry ? Theme.error : Theme.border

        ColumnLayout {
            id: content
            anchors.fill: parent
            anchors.margins: Theme.pad
            spacing: Theme.gap

            Text {
                Layout.fillWidth: true
                text: "Administrator password"
                color: Theme.fg
                font.pixelSize: Theme.fontSizeTitle
                font.weight: Font.DemiBold
            }

            Text {
                Layout.fillWidth: true
                text: root.prompt
                color: Theme.fgDim
                font.pixelSize: Theme.fontSize
                font.family: Theme.monoFamily
                wrapMode: Text.WordWrap
            }

            Text {
                Layout.fillWidth: true
                visible: root.retry
                text: "That password was not accepted."
                color: Theme.error
                font.pixelSize: Theme.fontSize
                wrapMode: Text.WordWrap
            }

            TextField {
                id: field

                Layout.fillWidth: true
                implicitHeight: Theme.rowHeight
                leftPadding: Theme.gap
                rightPadding: Theme.gap
                echoMode: TextInput.Password
                passwordCharacter: "•"
                color: Theme.fg
                selectionColor: Theme.accent
                selectedTextColor: Theme.onAccent
                font.pixelSize: Theme.fontSize

                background: Rectangle {
                    radius: Theme.radiusSmall
                    color: Theme.bgAlt
                    border.width: 1
                    border.color: field.activeFocus ? Theme.accent : Theme.border
                }

                Keys.onReturnPressed: root.submit()
                Keys.onEnterPressed: root.submit()
                Keys.onEscapePressed: root.refuse()
            }

            Text {
                Layout.fillWidth: true
                text: "hyprpm needs this to write its plugin store and the Hyprland headers. "
                      + "It goes to sudo and nowhere else, and is kept only until this "
                      + "operation finishes, so that one click does not ask three times."
                color: Theme.muted
                font.pixelSize: Theme.fontSizeSmall
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
                    onClicked: root.refuse()
                }

                PillButton {
                    text: "Authenticate"
                    primary: true
                    onClicked: root.submit()
                }
            }
        }
    }

    Keys.onEscapePressed: root.refuse()
    focus: visible
}
