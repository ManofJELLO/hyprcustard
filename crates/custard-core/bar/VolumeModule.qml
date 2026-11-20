import QtQuick
import Quickshell
import Quickshell.Services.Pipewire

// The default output's volume. Click mutes, scroll changes it by 1%,
// right-click opens the mixer the Bar page names.
Module {
    id: vol

    readonly property var sink: Pipewire.defaultAudioSink
    PwObjectTracker { objects: vol.sink ? [vol.sink] : [] }

    readonly property bool ready: sink !== null && sink.audio !== null
    readonly property int percent: ready ? Math.round(sink.audio.volume * 100) : 0
    readonly property bool muted: ready && sink.audio.muted
    readonly property var icons: ["", "", ""]

    // With no output it shows as muted rather than hiding, so the modules
    // beside it never shift.
    text: Config.glyphs
          ? (muted || !ready ? " " + String.fromCodePoint(0xf0581) + " "
                   : percent + " " + icons[Math.min(2, Math.floor(percent / 33))])
          : (!ready ? "No audio" : muted ? "Muted" : "Vol " + percent + "%")
    minText: Config.glyphs ? "100  " : "Vol 100%"
    tooltip: vol.sink?.description ?? "No audio output"
    padLeft: 16.5
    padRight: 11

    buttons: Qt.LeftButton | Qt.RightButton
    onClicked: mouse => {
        if (mouse.button === Qt.RightButton) Config.run(Config.mixer);
        else if (ready) sink.audio.muted = !sink.audio.muted;
    }
    onScrolled: wheel => {
        if (!ready || wheel.angleDelta.y === 0) return;
        const step = wheel.angleDelta.y > 0 ? 0.01 : -0.01;
        sink.audio.volume = Math.max(0, Math.min(1, sink.audio.volume + step));
    }
}
