pragma Singleton
import QtQuick

// Tema fixo para os testes: o de verdade lê arquivos do Omarchy via Quickshell.
QtObject {
    property color accent: "orange"
    property color surface: "gray"
    property color foreground: "white"
    property color muted: "gray"
    property color background: "black"
    property color urgent: "red"
    property string font: "monospace"
    readonly property int radius: 6
}
