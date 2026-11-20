import QtQuick
import Quickshell.Io

// Wired, Wi-Fi or offline, from the default route. The network's name comes
// from NetworkManager, iwd or iw, whichever is there, and the signal from
// /proc/net/wireless, so it works without any one network daemon.
Module {
    id: net

    property string kind: "none"
    property string ifname: ""
    property string ssid: ""
    property int strength: 0
    property string address: ""

    text: Config.glyphs
          ? (kind === "wifi" ? "" : kind === "ethernet" ? String.fromCodePoint(0xf0200)
             : String.fromCodePoint(0xf05aa))
          : (kind === "wifi" ? "Wi-Fi" : kind === "ethernet" ? "Wired" : "Offline")
    minText: Config.glyphs ? "" : "Offline"
    tooltip: kind === "wifi" ? (ssid !== "" ? ssid + "  " : "") + "Signal: " + strength + "%"
           : kind === "ethernet" ? ifname + "  " + address
           : "Disconnected"
    padLeft: 11
    padRight: 22

    Process {
        id: probe
        command: ["sh", "-c", `
            dev=$(ip route show default 2>/dev/null | awk '{print $5; exit}')
            [ -z "$dev" ] && { echo none; exit; }
            addr=$(ip -4 -o addr show "$dev" 2>/dev/null | awk '{print $4; exit}')
            if [ -d "/sys/class/net/$dev/wireless" ]; then
                ssid=""
                if command -v nmcli >/dev/null 2>&1; then
                    ssid=$(nmcli -t -f active,ssid dev wifi 2>/dev/null | sed -n 's/^yes://p' | head -n1)
                fi
                if [ -z "$ssid" ] && command -v iwctl >/dev/null 2>&1; then
                    ssid=$(iwctl station "$dev" show 2>/dev/null | sed 's/\\x1b\\[[0-9;]*m//g' \\
                        | sed -n 's/^ *Connected network  *//p' | sed 's/ *$//')
                fi
                if [ -z "$ssid" ] && command -v iw >/dev/null 2>&1; then
                    ssid=$(iw dev "$dev" link 2>/dev/null | sed -n 's/^[[:space:]]*SSID: //p')
                fi
                level=$(awk -v d="$dev:" '$1 == d { gsub(/\\./, "", $4); print $4 }' /proc/net/wireless)
                printf 'wifi\\t%s\\t%s\\t%s\\t%s\\n' "$dev" "$addr" "$ssid" "$level"
            else
                printf 'ethernet\\t%s\\t%s\\n' "$dev" "$addr"
            fi
        `]
        stdout: StdioCollector {
            onStreamFinished: {
                const f = text.trim().split("\t");
                net.kind = f[0] || "none";
                net.ifname = f[1] ?? "";
                net.address = f[2] ?? "";
                net.ssid = f[3] ?? "";
                // -90 dBm is 0%, -45 dBm is 100%.
                const dbm = Number(f[4]);
                net.strength = isNaN(dbm) || f[4] === undefined || f[4] === "" ? 0
                    : Math.max(0, Math.min(100, Math.round((dbm + 90) / 45 * 100)));
            }
        }
    }

    Timer {
        interval: 5000
        running: net.shown
        repeat: true
        triggeredOnStart: true
        onTriggered: probe.running = true
    }
}
