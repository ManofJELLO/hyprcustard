import QtQuick
import QtQuick.Layouts
import dev.hyprcustard.gui

// Pick a shader from the shader folder, with an optional duration override and
// an optional "fallback" switch for tags that support `_default`.
//
// Emits one `changed` with the whole new value, so the caller sends one command.
RowLayout {
    id: picker

    property string path: ""
    property var duration: null          // null means "use the shader's own"
    property bool isDefault: false
    property bool showDuration: false
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

    LineEdit {
        id: durationField
        visible: picker.showDuration && picker.path !== ""
        Layout.preferredWidth: 86
        mono: true
        placeholderText: {
            var s = App.shaderByPath(picker.path)
            if (s && s.duration !== null && s.duration !== undefined)
                return String(Math.round(s.duration * 1000) / 1000) + "s"
            return "0.3s"
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
