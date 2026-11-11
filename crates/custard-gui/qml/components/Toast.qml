import QtQuick
import dev.hyprcustard.gui

// Transient status messages, newest at the bottom.
Item {
    id: toast

    property int lifetime: 5200

    function show(message, isError) {
        toastModel.append({ message: String(message), isError: isError === true })
        if (toastModel.count > 4)
            toastModel.remove(0)
    }

    ListModel {
        id: toastModel
    }

    Column {
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.margins: Theme.pad
        spacing: Theme.gapSmall

        Repeater {
            model: toastModel

            Rectangle {
                id: item

                required property string message
                required property bool isError
                required property int index

                width: Math.min(toast.width - Theme.pad * 2, Math.max(240, label.implicitWidth + Theme.pad * 2))
                height: label.implicitHeight + Theme.pad
                radius: Theme.radius
                color: Theme.surface
                border.width: 1
                border.color: item.isError ? Theme.error : Theme.border
                opacity: 0

                Rectangle {
                    width: 3
                    height: parent.height - 2
                    anchors.left: parent.left
                    anchors.leftMargin: 1
                    anchors.verticalCenter: parent.verticalCenter
                    radius: 2
                    color: item.isError ? Theme.error : Theme.ok
                }

                Text {
                    id: label
                    anchors.fill: parent
                    anchors.margins: Theme.gap
                    anchors.leftMargin: Theme.gap + 4
                    text: item.message
                    color: Theme.fg
                    font.pixelSize: Theme.fontSize
                    wrapMode: Text.WordWrap
                    verticalAlignment: Text.AlignVCenter
                }

                MouseArea {
                    anchors.fill: parent
                    onClicked: toastModel.remove(item.index)
                }

                SequentialAnimation {
                    running: true
                    NumberAnimation {
                        target: item
                        property: "opacity"
                        to: 1
                        duration: 140
                    }
                    PauseAnimation {
                        duration: toast.lifetime
                    }
                    NumberAnimation {
                        target: item
                        property: "opacity"
                        to: 0
                        duration: 260
                    }
                    ScriptAction {
                        script: {
                            if (item.index >= 0 && item.index < toastModel.count)
                                toastModel.remove(item.index)
                        }
                    }
                }
            }
        }
    }
}
