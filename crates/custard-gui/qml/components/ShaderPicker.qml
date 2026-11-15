import QtQuick
import QtQuick.Layouts
import dev.hyprcustard.gui

// Pick a shader from the shader folder, with an optional `@seconds` override and
// an optional "fallback" switch for tags that support `_default`. What the
// seconds set depends on the tag: `secondsMeaning` is "duration" for a one-shot
// animation, or "settle" for a motion one, whose own clock is Hyprland's
// animation and whose seconds are how long it keeps playing after it stops.
//
// Emits one `changed` with the whole new value, so the caller sends one command.
RowLayout {
    id: picker

    property string path: ""
    property var duration: null          // null means "use the shader's own"
    property bool isDefault: false
    property bool showDuration: false
    property string secondsMeaning: "duration"
    property bool showDefault: false
    property string placeholder: "not set"

    signal picked(string path, var duration, bool isDefault)

    spacing: Theme.gapSmall

    readonly property var _options: {
        var out = [{ path: "", label: picker.placeholder }]
        var list = App.shaders
        for (var i = 0; i < list.length; ++i)
            out.push({ path: list[i].path, label: list[i].name + "  ·  " + App.fileName(list[i].path) })
        // A path the config refers to that is not in the folder still has to be
        // selectable, or opening the editor would silently drop it.
        if (picker.path !== "" && App.shaderByPath(picker.path) === null)
            out.push({ path: picker.path, label: App.fileName(picker.path) + "  (missing)" })
        return out
    }

    function _indexOfPath(p) {
        for (var i = 0; i < _options.length; ++i)
            if (_options[i].path === p)
                return i
        return 0
    }

    Dropdown {
        id: combo
        Layout.fillWidth: true
        model: picker._options
        textRole: "label"
        currentIndex: picker._indexOfPath(picker.path)
        onActivated: function (index) {
            var p = picker._options[index].path
            picker.picked(p, p === "" ? null : picker.duration, picker.isDefault)
        }
    }

    Text {
        visible: durationField.visible && picker.secondsMeaning === "settle"
        text: "settle"
        color: Theme.muted
        font.pixelSize: Theme.fontSizeSmall
    }

    LineEdit {
        id: durationField
        visible: picker.showDuration && picker.path !== ""
        Layout.preferredWidth: 86
        mono: true
        // The shader's own value, else the plugin's: 0.3 s for a duration,
        // none for a settle tail.
        placeholderText: {
            var s = App.shaderByPath(picker.path)
            var own = s ? (picker.secondsMeaning === "settle" ? s.settle : s.duration) : null
            if (own !== null && own !== undefined)
                return String(Math.round(own * 1000) / 1000) + "s"
            return picker.secondsMeaning === "settle" ? "0s" : "0.3s"
        }
        // A 32-bit float widened to a JSON double shows up as 0.20000000298;
        // three decimals is all the plugin's @sec override is worth anyway.
        text: picker.duration === null || picker.duration === undefined
              ? "" : String(Math.round(picker.duration * 1000) / 1000)
        onCommit: function (v) {
            var t = v.trim().replace(/s$/, "")
            if (t === "") {
                picker.picked(picker.path, null, picker.isDefault)
                return
            }
            var parsed = parseFloat(t)
            if (isNaN(parsed) || parsed < 0) {
                text = picker.duration === null ? "" : String(picker.duration)
                return
            }
            picker.picked(picker.path, Math.min(parsed, 5), picker.isDefault)
        }
    }

    Toggle {
        id: fallback
        visible: picker.showDefault && picker.path !== ""
        text: "fallback"
        checked: picker.isDefault
        onToggled: picker.picked(picker.path, picker.duration, checked)
    }
}
