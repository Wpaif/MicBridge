pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io

// Paleta do tema Omarchy atual (colors.toml), relida quando o tema muda.
// Fora do Omarchy fica a paleta escura padrão abaixo.
Singleton {
    id: root

    property color background: "#101315"
    property color surface: Qt.lighter(background, 1.35)
    property color foreground: "#cacccc"
    property color muted: Qt.darker(foreground, 1.6)
    property color accent: "#7aa2f7"
    property color urgent: "#d35f5f"
    property color ok: "#8fbf7f"
    property color warn: "#e6b450"
    property string font: "JetBrainsMono Nerd Font"
    readonly property int radius: 6
    readonly property int gap: 12

    function apply(text) {
        var values = {}
        var lines = String(text).split("\n")
        for (var i = 0; i < lines.length; i++) {
            var m = lines[i].match(/^\s*([a-z_]+)\s*=\s*"(#[0-9a-fA-F]{6,8})"/)
            if (m) values[m[1]] = m[2]
        }
        if (values.background) root.background = values.background
        if (values.lighter_background) root.surface = values.lighter_background
        if (values.foreground) root.foreground = values.foreground
        if (values.light_foreground) root.muted = values.light_foreground
        if (values.accent) root.accent = values.accent
        if (values.red) root.urgent = values.red
    }

    FileView {
        path: Quickshell.env("HOME") + "/.local/state/omarchy/current/theme/colors.toml"
        watchChanges: true
        printErrors: false
        onFileChanged: reload()
        onLoaded: root.apply(text())
    }
}
