import QtQuick
import QtQuick.Controls.Basic
import dev.hyprcustard.gui

// A text field that looks like the rest of the app and commits on edit, so a
// value is never lost because the field still had focus when Save was pressed.
TextField {
    id: control

    property bool mono: false
    signal commit(string value)

    implicitHeight: Theme.rowHeight
    leftPadding: Theme.gap
    rightPadding: Theme.gap
    color: Theme.fg
    placeholderTextColor: Theme.muted
    selectionColor: Theme.accent
    selectedTextColor: Theme.onAccent
    font.pixelSize: Theme.fontSize
    font.family: control.mono ? Theme.monoFamily : font.family

    background: Rectangle {
        radius: Theme.radiusSmall
        color: Theme.bgAlt
        border.width: 1
        border.color: control.activeFocus ? Theme.accent : Theme.border
    }

    onEditingFinished: control.commit(control.text)
    Keys.onReturnPressed: control.commit(control.text)
    Keys.onEnterPressed: control.commit(control.text)
}
