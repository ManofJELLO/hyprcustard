import QtQuick
import Quickshell.Io

// Memory in use, every second; the tooltip gives it in GiB.
Module {
    id: mem

    property int percent: 0
    property real usedGiB: 0

    text: Config.glyphs ? percent + "% " + String.fromCodePoint(0xf035b) : "RAM " + percent + "%"
    minText: Config.glyphs ? "100% " + String.fromCodePoint(0xf035b) : "RAM 100%"
    tooltip: usedGiB.toFixed(1) + " GiB used"
    padRight: 11

    FileView {
        id: meminfo
        path: "/proc/meminfo"
        onLoaded: {
            const t = text();
            const kb = key => Number((t.match(new RegExp("^" + key + ":\\s+(\\d+)", "m")) ?? [0, 0])[1]);
            const total = kb("MemTotal"), avail = kb("MemAvailable");
            if (total > 0) {
                mem.percent = Math.round((total - avail) / total * 100);
                mem.usedGiB = (total - avail) / 1048576;
            }
        }
    }

    Timer {
        interval: 1000
        running: mem.shown
        repeat: true
        triggeredOnStart: true
        onTriggered: meminfo.reload()
    }
}
