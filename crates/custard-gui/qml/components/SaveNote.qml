import QtQuick
import QtQuick.Layouts
import dev.hyprcustard.gui

// One line at the top of a page saying how its changes are kept.
//
// The app has three ways of saving: Hyprland's own settings wait for the
// header's Save, some pages have a Save of their own, and the rest write
// the moment something is clicked. Without this a page of the third kind
// looks like it is waiting for a Save that never comes, or the header's
// Save looks like it covers a page it does not.
RowLayout {
    id: note

    // What happens here, in a sentence.
    property string text: "Changes here are saved as you make them. The Save button at the "
                          + "top is only for the Hyprland pages."
    // The badge: how this page saves, in two or three words.
    property string label: "Saves at once"

    spacing: Theme.gap

    Badge {
        Layout.alignment: Qt.AlignTop
        text: note.label
        tint: Theme.accentAlt
    }
    Text {
        Layout.fillWidth: true
        text: note.text
        color: Theme.muted
        font.pixelSize: Theme.fontSizeSmall
        wrapMode: Text.WordWrap
    }
}
