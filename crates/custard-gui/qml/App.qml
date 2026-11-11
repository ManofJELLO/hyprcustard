pragma Singleton

import QtQuick

// This file is part of the dev.hyprcustard.gui module, so the module's own
// types (Backend) are available without importing it. Importing it here would
// make the two singletons depend on each other through the module.

// The parsed application state, plus the handful of helpers every page needs.
// Nothing here decides anything — the engine does that; this is a view onto it.
QtObject {
    id: app

    // Both of these are wired up by Main.qml. The singleton does not reach for
    // Backend itself: a singleton that imports its own module makes the module's
    // singletons depend on each other, which QML refuses to load.
    property string rawState: "{}"
    property var backend: null
    property var confirmDialog: null

    readonly property var state: {
        try {
            return JSON.parse(app.rawState)
        } catch (e) {
            return ({})
        }
    }

    readonly property bool ready: app.backend ? app.backend.ready : false

    // Two separate kinds of unsaved work, because they are written to different
    // files by different buttons: `dirty` is the managed block in your Hyprland
    // config, `dirtyShaders` is how many .glsl files have staged edits.
    readonly property bool dirty: state.dirty === true
    readonly property int dirtyShaders: state.dirtyShaders || 0

    readonly property var rules: state.rules || []
    readonly property var layers: state.layers || []
    readonly property var binds: state.binds || []
    readonly property var startup: state.startup || []
    readonly property var shaders: state.shaders || []
    readonly property var missingShaders: state.missingShaders || []
    readonly property var tagCatalog: state.tagCatalog || []
    readonly property var uniformCatalog: state.uniformCatalog || []
    readonly property var themes: state.themes || []
    readonly property var problems: state.problems || []
    readonly property var settings: state.settings || ({})
    readonly property var live: state.live || ({})

    readonly property bool hyprlandRunning: live.running === true
    readonly property bool pluginLoaded: live.pluginLoaded === true
    readonly property var liveClasses: live.classes || []
    readonly property var liveNamespaces: live.namespaces || []

    readonly property string configPathDisplay: state.configPathDisplay || ""
    readonly property string shaderDirDisplay: state.shaderDirDisplay || ""
    readonly property string appVersion: state.appVersion || ""

    // --- commands -----------------------------------------------------------

    function run(command, payload) {
        if (!app.backend)
            return
        app.backend.run(command, JSON.stringify(payload === undefined ? {} : payload))
    }

    function query(what, payload) {
        if (!app.backend)
            return "{}"
        return app.backend.query(what, JSON.stringify(payload === undefined ? {} : payload))
    }

    function queryJson(what, payload) {
        try {
            return JSON.parse(query(what, payload))
        } catch (e) {
            return ({ error: "the backend returned something unreadable" })
        }
    }

    readonly property var look: state.look || ({ options: [], groups: [] })

    // Live-only: puts a value on the desktop mid-drag without storing it.
    function lookTry(key, value) {
        if (!app.backend)
            return ""
        return app.backend.lookTry(key, JSON.stringify(value))
    }

    function lookSet(key, value) {
        app.run("look.set", { key: key, value: value })
    }

    function refresh() {
        if (app.backend)
            app.backend.refresh()
    }

    // --- hyprpm -------------------------------------------------------------
    //
    // The plugin manager is the one thing the app drives that takes minutes
    // and can stop to ask for a password, so it has its own small surface
    // rather than going through `run`.

    readonly property bool hyprpmBusy: app.backend ? app.backend.hyprpmBusy : false
    readonly property string hyprpmLabel: app.backend ? app.backend.hyprpmLabel : ""

    // `op` is add, remove, enable, disable, update or reload. The argument is
    // the repository URL or the plugin name, for the ones that need one.
    function hyprpm(op, argument) {
        if (app.backend)
            app.backend.hyprpmRun(op, argument === undefined ? "" : argument)
    }

    function hyprpmCancel() {
        if (app.backend)
            app.backend.hyprpmCancel()
    }

    function hyprpmInTerminal(op, argument) {
        if (app.backend)
            app.backend.hyprpmOpenTerminal(op, argument === undefined ? "" : argument)
    }

    // What hyprpm has installed. Cheap enough to ask for directly.
    function hyprpmStatus() {
        if (!app.backend)
            return ({ installed: false, plugins: [] })
        try {
            return JSON.parse(app.backend.hyprpmStatus())
        } catch (e) {
            return ({ installed: false, plugins: [] })
        }
    }

    // --- preview ------------------------------------------------------------
    //
    // A second Hyprland with the plugin loaded into it, rendering a temporary
    // copy of the shader being edited. Frames come back as a file to point an
    // Image at, not as data.

    readonly property bool previewBusy: app.backend ? app.backend.previewBusy : false
    readonly property bool previewRunning: app.backend ? app.backend.previewRunning : false
    readonly property string previewShader: app.backend ? app.backend.previewShader : ""

    // `background` is what the preview clears to, so the window in it looks
    // like it is sitting on this pane rather than on a desktop of its own.
    function previewStart(path, width, height, background, desktopBackdrop) {
        if (app.backend)
            app.backend.previewStart(path, Math.round(width), Math.round(height),
                                     String(background), desktopBackdrop === true)
    }

    function previewStop() {
        if (app.backend)
            app.backend.previewStop()
    }

    // Push the staged values into the running preview. Cheap, and a no-op when
    // nothing is running.
    function previewUpdate(path) {
        if (app.backend)
            app.backend.previewUpdate(path)
    }

    // { path, n } for a frame, or { error } when there is nothing to grab.
    function previewFrame() {
        if (!app.backend)
            return ({ error: "no backend" })
        try {
            return JSON.parse(app.backend.previewFrame())
        } catch (e) {
            return ({ error: "unreadable frame" })
        }
    }

    // Asks the compositor what is on screen without blocking the interface.
    // The answer lands in `state` when it arrives.
    function refreshLive() {
        if (app.backend)
            app.backend.refreshLive()
    }

    // Ask before doing something that rewrites a file or throws work away.
    // With no dialog wired up the action still happens, so a page is never
    // left with a button that quietly does nothing.
    // Like confirm, with a second answer besides the main one.
    function choose(heading, body, acceptText, danger, onAccept, altText, onAlt) {
        if (app.confirmDialog)
            app.confirmDialog.choose(heading, body, acceptText, danger, onAccept, altText, onAlt)
    }

    function confirm(heading, body, acceptText, danger, onAccept) {
        if (app.confirmDialog)
            app.confirmDialog.ask(heading, body, acceptText, danger, onAccept)
        else
            onAccept()
    }

    // --- lookups ------------------------------------------------------------

    function ruleById(id) {
        for (var i = 0; i < rules.length; ++i)
            if (rules[i].id === id)
                return rules[i]
        return null
    }

    function layerById(id) {
        for (var i = 0; i < layers.length; ++i)
            if (layers[i].id === id)
                return layers[i]
        return null
    }

    function shaderByPath(path) {
        if (!path)
            return null
        for (var i = 0; i < shaders.length; ++i)
            if (shaders[i].path === path)
                return shaders[i]
        return null
    }

    function tagInfo(key) {
        for (var i = 0; i < tagCatalog.length; ++i)
            if (tagCatalog[i].key === key || tagCatalog[i].slot === key)
                return tagCatalog[i]
        return null
    }

    // Tags grouped for the rule editor, in catalog order.
    readonly property var tagGroups: {
        var order = []
        var byGroup = ({})
        for (var i = 0; i < tagCatalog.length; ++i) {
            var t = tagCatalog[i]
            if (byGroup[t.group] === undefined) {
                byGroup[t.group] = { group: t.group, label: t.groupLabel, tags: [] }
                order.push(byGroup[t.group])
            }
            byGroup[t.group].tags.push(t)
        }
        return order
    }

    // --- formatting ---------------------------------------------------------

    function fileName(path) {
        if (!path)
            return ""
        var parts = String(path).split("/")
        return parts[parts.length - 1]
    }

    // "3 shaders", "1 shader".
    function plural(n, word) {
        return n + " " + word + (n === 1 ? "" : "s")
    }

    function seconds(v) {
        if (v === null || v === undefined)
            return ""
        return (Math.round(v * 1000) / 1000) + "s"
    }

    // A rule's tag for one slot, or null.
    function tagOf(rule, key) {
        if (!rule || !rule.tags)
            return null
        for (var i = 0; i < rule.tags.length; ++i)
            if (rule.tags[i].key === key)
                return rule.tags[i]
        return null
    }
}
