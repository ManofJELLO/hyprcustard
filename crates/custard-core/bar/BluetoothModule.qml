import QtQuick
import Quickshell
import Quickshell.Bluetooth

// Off, on, or how many devices are connected; click opens the Bluetooth app
// the Bar page names. With no adapter it shows as off rather than hiding,
// so the modules beside it never shift (turn it off on the Bar page if the
// machine has no Bluetooth).
Module {
    id: bt

    readonly property var adapter: Bluetooth.defaultAdapter
    readonly property bool on: adapter !== null && adapter.enabled
    readonly property var connected: adapter ? adapter.devices.values.filter(d => d.connected) : []

    text: Config.glyphs
          ? (!on ? String.fromCodePoint(0xf00b2)
             : connected.length > 0 ? " " + connected.length : "")
          : (!on ? "BT off" : connected.length > 0 ? "BT " + connected.length : "BT")
    minText: Config.glyphs ? " 9" : "BT off"
    tooltip: adapter === null ? "No Bluetooth adapter found"
           : !on ? "Bluetooth is off"
           : connected.length > 0 ? connected.map(d => d.name).join("\n") : "Bluetooth is on"

    buttons: Config.bluetoothApp !== "" ? Qt.LeftButton : Qt.NoButton
    onClicked: Config.run(Config.bluetoothApp)
}
