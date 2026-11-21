import QtQuick
import Quickshell
import Quickshell.Io

// A temperature in °C, every two seconds, from the sensor temps.sh finds.
// With no sensor it hides itself, once, at start. A GPU that is asleep
// shows "off" and is not woken to be read.
Module {
    id: t

    // "cpu" or "gpu".
    required property string kind
    property int glyph: 0

    // What temps.sh found: "file" (a kernel sensor), "nvidia" (nvidia-smi)
    // or "" for nothing.
    property string how: ""
    property string input: ""
    property string device: ""
    property string sensor: ""
    property bool searched: false
    property int celsius: -1
    property bool asleep: false
    property int failures: 0

    readonly property string name: kind === "gpu" ? "GPU" : "CPU"
    readonly property string reading: asleep ? "off" : celsius >= 0 ? celsius + "°C" : "–"

    // Kept in place while it looks, so finding a sensor moves nothing.
    visible: shown && (!searched || how !== "")
    text: Config.glyphs ? reading + " " + String.fromCodePoint(glyph) : name + " " + reading
    minText: Config.glyphs ? "100°C " + String.fromCodePoint(glyph) : name + " 100°C"
    tooltip: name + " temperature" + (sensor !== "" ? " (" + sensor + ")" : "")
             + (asleep ? "\nThe GPU is asleep, and is left so." : "")
    padLeft: 11
    padRight: 11

    function search() {
        if (t.shown && !finder.running)
            finder.running = true;
    }
    onShownChanged: if (!t.searched) t.search()
    Component.onCompleted: t.search()

    function tick() {
        if (t.device !== "")
            power.reload();
        else
            t.read();
    }
    function read() {
        if (t.how === "file")
            temp.reload();
        else if (t.how === "nvidia" && !smi.running)
            smi.running = true;
    }
    function failed() {
        t.celsius = -1;
        // The sensor may have moved (a driver reloaded); look again now and
        // then rather than every tick.
        if (t.failures++ % 15 === 0)
            t.search();
    }

    Process {
        id: finder
        command: ["sh", Quickshell.shellPath("temps.sh"), t.kind]
        stdout: StdioCollector {
            onStreamFinished: {
                const f = text.split("\n")[0].split("\t");
                t.how = f[0] === "file" || f[0] === "nvidia" ? f[0] : "";
                t.input = t.how === "file" ? f[1] : "";
                t.device = f[2] && f[2] !== "-" ? f[2] : "";
                t.sensor = f[3] ?? "";
                t.searched = true;
                if (t.how !== "")
                    t.tick();
            }
        }
    }

    FileView {
        id: temp
        path: t.input
        printErrors: false
        onLoaded: {
            const v = parseInt(text());
            if (isNaN(v)) {
                t.failed();
            } else {
                t.celsius = Math.round(v / 1000);
                t.failures = 0;
            }
        }
        onLoadFailed: t.failed()
    }

    // Whether the GPU is awake: reading a sleeping one through nvidia-smi
    // would wake it, which costs a laptop its battery.
    FileView {
        id: power
        path: t.device !== "" ? t.device + "/power/runtime_status" : ""
        printErrors: false
        onLoaded: {
            t.asleep = text().trim() === "suspended";
            if (!t.asleep)
                t.read();
        }
        onLoadFailed: {
            t.asleep = false;
            t.read();
        }
    }

    Process {
        id: smi
        command: ["nvidia-smi", "--query-gpu=temperature.gpu", "--format=csv,noheader,nounits"]
        stdout: StdioCollector {
            onStreamFinished: {
                const v = parseInt(text.split("\n")[0]);
                if (isNaN(v)) {
                    t.failed();
                } else {
                    t.celsius = v;
                    t.failures = 0;
                }
            }
        }
    }

    Timer {
        interval: 2000
        running: t.shown && t.how !== ""
        repeat: true
        onTriggered: t.tick()
    }
}
