import QtQuick
import QtQuick.Controls.Basic
import dev.hyprcustard.gui

ComboBox {
    id: control

    implicitHeight: Theme.rowHeight
    font.pixelSize: Theme.fontSize

    delegate: ItemDelegate {
        id: item

        required property int index

        width: control.width
        highlighted: control.highlightedIndex === item.index

        contentItem: Text {
            // `textAt` is the only lookup that is right for every model shape,
            // and it is what `displayText` already uses — so the open list and
            // the closed box cannot disagree.
            //
            // Reaching into `modelData` or `model` by hand is not right:
            // assigning a JS array to `model` converts it to a QVariantList,
            // so `Array.isArray` reads back false, and a model of objects then
            // resolves to nothing at all. That looks like a list of blank rows
            // that still select the correct entry.
            text: control.textAt(item.index)
            color: item.highlighted ? Theme.accent : Theme.fg
            font.pixelSize: Theme.fontSize
            elide: Text.ElideRight
            verticalAlignment: Text.AlignVCenter
        }
        background: Rectangle {
            color: item.highlighted ? Theme.wash(Theme.accent, 0.16) : "transparent"
            radius: 4
        }
    }

    indicator: Text {
        x: control.width - width - Theme.gap
        y: control.topPadding + (control.availableHeight - height) / 2
        text: "▾"
        color: Theme.muted
        font.pixelSize: Theme.fontSize
    }

    contentItem: Text {
        leftPadding: Theme.gap
        rightPadding: control.indicator.width + Theme.gap
        text: control.displayText
        color: control.enabled ? Theme.fg : Theme.muted
        font.pixelSize: Theme.fontSize
        verticalAlignment: Text.AlignVCenter
        elide: Text.ElideRight
    }

    background: Item {
        Raised {
            anchors.fill: parent
            visible: !control.down && !control.popup.visible
            radius: Theme.radiusSmall
            depth: Theme.depthSmall
            glow: control.visualFocus
        }
        Inset {
            anchors.fill: parent
            visible: control.down || control.popup.visible
            radius: Theme.radiusSmall
            glow: true
        }
    }

    popup: Popup {
        y: control.height + 2
        width: control.width
        implicitHeight: Math.min(contentItem.implicitHeight + 2, 320)
        padding: 1

        contentItem: ListView {
            id: options
            clip: true
            implicitHeight: contentHeight
            model: control.popup.visible ? control.delegateModel : null
            currentIndex: control.highlightedIndex
            // Always drawn when the list is longer than the popup, so it is
            // plain there is more to scroll to before anyone tries.
            ScrollBar.vertical: ScrollBar {
                policy: options.contentHeight > options.height ? ScrollBar.AlwaysOn : ScrollBar.AlwaysOff
            }
        }

        background: Raised {
            radius: Theme.radiusSmall
            depth: Theme.depthSmall
            edge: Theme.wash(Theme.accent, 0.5)
        }
    }
}
