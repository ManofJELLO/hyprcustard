import QtQuick
import QtQuick.Controls.Basic

// A page's scrolling column of cards. It does not clip, so the cards'
// shadows fade out instead of being cut off at the page's edge; the header
// and the sidebar in Main.qml are drawn over whatever scrolls past them.
ScrollView {
    id: scroll

    clip: false
    contentWidth: scroll.availableWidth

    // The Flickable that ScrollView makes for its content clips on its own.
    Component.onCompleted: scroll.contentItem.clip = false
}
