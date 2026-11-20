import QtQuick
import Quickshell

// The date and time, in the Bar page's format; a month calendar on hover.
Module {
    id: clock

    padLeft: 11
    padRight: 11
    edgeRight: 3
    edgeColor: Theme.yellow

    SystemClock {
        id: sys
        enabled: clock.shown
        precision: Config.clockFormat.indexOf("s") >= 0 ? SystemClock.Seconds : SystemClock.Minutes
    }

    text: " " + Qt.formatDateTime(sys.date, Config.clockFormat) + " "

    tooltipRich: true
    tooltip: {
        const d = sys.date;
        const y = d.getFullYear(), mo = d.getMonth(), today = d.getDate();
        const first = new Date(y, mo, 1).getDay();
        const days = new Date(y, mo + 1, 0).getDate();
        let cells = [];
        for (let i = 0; i < first; i++) cells.push("  ");
        for (let day = 1; day <= days; day++) {
            const s = String(day).padStart(2, " ");
            cells.push(day === today ? "<b><u>" + s + "</u></b>" : s);
        }
        let rows = ["Su Mo Tu We Th Fr Sa"];
        for (let i = 0; i < cells.length; i += 7) rows.push(cells.slice(i, i + 7).join(" "));
        return "<big>" + y + " " + Qt.formatDateTime(d, "MMMM") + "</big>"
            + "<pre style='font-size:10px'>" + rows.join("\n") + "</pre>";
    }
}
