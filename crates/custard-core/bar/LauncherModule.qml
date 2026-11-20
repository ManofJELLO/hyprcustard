import QtQuick

// Opens the app launcher.
Module {
    id: button

    required property var launcher

    text: Config.glyphs ? String.fromCodePoint(0xf003b) : "Apps"
    tooltip: "Apps"
    padLeft: 11
    padRight: 11
    edgeRight: 3
    edgeColor: Theme.aqua

    buttons: Qt.LeftButton
    onClicked: button.launcher.toggle()
}
