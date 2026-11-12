import QtQuick
import dev.hyprcustard.gui

// A line of text that is cut short with "…" when it does not fit, and shows
// the whole of it in a tip after a moment's hover.
Text {
    id: label

    elide: Text.ElideRight

    HoverHandler {
        id: hover
    }

    Tip {
        visible: hover.hovered && label.truncated
        delay: 500
        text: label.text
    }
}
