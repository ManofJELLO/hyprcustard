import QtQuick
import Quickshell.Services.Mpris

// What is playing. Click: play or pause. Double-click: next.
// Right double-click: previous. Hidden while nothing plays.
Module {
    id: media

    // The player the Bar page names, else one that is playing, else any.
    readonly property var player: {
        const all = Mpris.players.values;
        const want = Config.player.toLowerCase();
        if (want !== "") {
            const named = all.find(p => (p.identity ?? "").toLowerCase().includes(want)
                                        || (p.dbusName ?? "").toLowerCase().includes(want));
            if (named) return named;
        }
        return all.find(p => p.isPlaying) ?? all[0] ?? null;
    }

    readonly property string info: {
        if (!player) return "";
        const id = String(player.metadata?.["mpris:trackid"] ?? "");
        if (id.includes(":ad:")) return "Advertisement";
        const artist = player.trackArtist, title = player.trackTitle;
        if (artist && title) return artist + " - " + title;
        return title ?? "";
    }

    visible: shown && info !== ""
    text: (Config.glyphs ? (player?.isPlaying ? "  " : "  ")
                         : (player?.isPlaying ? "⏸  " : "▶  ")) + info + "  "
    tooltip: (player?.identity ?? "") + (player?.identity ? ": " : "") + info
    maxTextWidth: 450
    padLeft: 11
    padRight: 16.5
    edgeRight: 3
    edgeColor: Theme.blue

    buttons: Qt.LeftButton | Qt.RightButton

    // Wait out the double-click window so a double-click does not also pause.
    Timer {
        id: single
        interval: Qt.styleHints.mouseDoubleClickInterval
        onTriggered: media.player?.togglePlaying()
    }
    onClicked: mouse => { if (mouse.button === Qt.LeftButton) single.restart(); }
    onDoubleClicked: mouse => {
        single.stop();
        if (mouse.button === Qt.LeftButton) player?.next();
        else player?.previous();
    }
}
