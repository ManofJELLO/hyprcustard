import QtQuick
import Quickshell.Io

// CPU use, every second.
Module {
    id: cpu

    property real usage: 0
    property var last: null

    text: Config.glyphs ? Math.round(usage) + "% " : "CPU " + Math.round(usage) + "%"
    minText: Config.glyphs ? "100% " : "CPU 100%"
    padLeft: 16.5
    padRight: 11
    edgeLeft: 3
    edgeColor: Theme.green

    FileView {
        id: stat
        path: "/proc/stat"
        onLoaded: {
            const f = text().split("\n")[0].trim().split(/\s+/).slice(1, 9).map(Number);
            const idle = f[3] + f[4];
            const total = f.reduce((a, b) => a + b, 0);
            if (cpu.last) {
                const dt = total - cpu.last.total;
                cpu.usage = dt > 0 ? 100 * (dt - (idle - cpu.last.idle)) / dt : 0;
            }
            cpu.last = { total: total, idle: idle };
        }
    }

    Timer {
        interval: 1000
        running: cpu.shown
        repeat: true
        onTriggered: stat.reload()
    }
}
