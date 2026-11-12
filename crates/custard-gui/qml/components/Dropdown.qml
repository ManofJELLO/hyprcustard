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
            color: Theme.fg
            font.pixelSize: Theme.fontSize
            elide: Text.ElideRight
            verticalAlignment: Text.AlignVCenter
        }
        background: Rectangle {
            color: item.highlighted ? Theme.selection : Theme.surface
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

    background: Rectangle {
        radius: Theme.radiusSmall
        color: Theme.bgAlt
        border.width: 1
        border.color: control.activeFocus ? Theme.accent : Theme.border
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

        background: Rectangle {
            color: Theme.surface
            border.width: 1
            border.color: Theme.border
            radius: Theme.radiusSmall
        }
    }
}
