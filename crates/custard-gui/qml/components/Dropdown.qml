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

    // Arrow keys move the highlight; follow it. A pointer moves it too, but
    // the row under a pointer is already in view, and scrolling then would
    // move the list under the cursor.
    onHighlightedIndexChanged: {
        if (!popup.opened || highlightedIndex < 0)
            return
        var row = optionRows.itemAt(highlightedIndex)
        if (row && !row.hovered)
            options.reveal(highlightedIndex)
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

    background: Raised {
        sunk: control.down || control.popup.visible
        radius: Theme.radiusSmall
        depth: sunk ? Theme.depthInset : Theme.depthSmall
        ring: control.visualFocus
    }

    popup: Popup {
        y: control.height + 2
        width: control.width
        implicitHeight: Math.min(contentItem.implicitHeight + 2, 320)
        padding: 1

        // A Flickable, not a ListView, on purpose: the list must never move
        // under the pointer. QQuickComboBox looks for an item view in its
        // popup and, whenever a row is hovered, scrolls that row fully into
        // view. A pointer coming down from the button first touches a row
        // that is only partly visible at the top edge; the list jumps to
        // show it, the next row slides under the still pointer, and so on
        // until the list has run back to the top. With no item view to find,
        // Qt leaves the scrolling to us: once when the list opens, and on
        // keyboard moves. Hover only highlights.
        contentItem: Flickable {
            id: options
            clip: true
            implicitHeight: contentHeight
            contentHeight: rows.implicitHeight
            boundsBehavior: Flickable.StopAtBounds

            // Scroll just enough to show row `i`, and no further.
            function reveal(i) {
                var row = optionRows.itemAt(i)
                if (!row || height <= 0)
                    return
                if (row.y < contentY)
                    contentY = row.y
                else if (row.y + row.height > contentY + height)
                    contentY = row.y + row.height - height
            }

            Column {
                id: rows
                width: options.width
                Repeater {
                    id: optionRows
                    model: control.popup.visible ? control.delegateModel : null
                }
            }

            // Always drawn when the list is longer than the popup, so it is
            // plain there is more to scroll to before anyone tries.
            ScrollBar.vertical: ScrollBar {
                policy: options.contentHeight > options.height ? ScrollBar.AlwaysOn : ScrollBar.AlwaysOff
            }
        }

        // Open with the chosen row at the top of the list, or as near the
        // top as the end of the list allows.
        onOpened: {
            // The rows are not placed yet when `opened` fires; every one
            // would report y = 0.
            rows.forceLayout()
            var row = optionRows.itemAt(control.currentIndex)
            options.contentY = row ? Math.max(0, Math.min(row.y, options.contentHeight - options.height)) : 0
        }

        background: Raised {
            radius: Theme.radiusSmall
            depth: Theme.depthSmall
            edge: Theme.wash(Theme.accent, 0.5)
        }
    }
}
