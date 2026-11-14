import QtQuick
import dev.hyprcustard.gui

// A shape pressed into the background: a darker floor, shaded along its top
// and left edges and lit along its bottom and right. Fields, tracks and wells
// are drawn on one.
Raised {
    sunk: true
    radius: Theme.radiusSmall
}
