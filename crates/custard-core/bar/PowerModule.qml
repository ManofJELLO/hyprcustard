import QtQuick

// Opens the power menu.
Module {
    id: button

    required property var menu

    text: Config.glyphs ? String.fromCodePoint(0xf0425) : "Power"
    tooltip: "Lock, log out, suspend, reboot or shut down"
    padLeft: 11
    padRight: 11
    edgeLeft: 3
    edgeColor: Theme.red

    buttons: Qt.LeftButton
    onClicked: button.menu.toggle()
}
