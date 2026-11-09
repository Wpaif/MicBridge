import QtQuick

// Barra de nível em dBFS (-60 a 0), com pico que decai devagar.
Item {
    id: root
    property real level: 0
    property string label: ""
    // Saturação recente: acende "CLIP" à direita da barra.
    property bool clip: false
    readonly property real fraction: level <= 0 ? 0 : Math.max(0, Math.min(1, (20 * Math.log(level) / Math.LN10 + 60) / 60))
    property real peak: 0
    onFractionChanged: if (fraction > peak) peak = fraction
    implicitHeight: 22

    Timer { interval: 50; running: root.peak > 0; repeat: true; onTriggered: root.peak = Math.max(root.fraction, root.peak - 0.01) }

    Text {
        id: caption
        text: root.label
        color: Theme.muted
        font.family: Theme.font
        font.pixelSize: 11
        width: 52
        anchors.verticalCenter: parent.verticalCenter
    }
    Text {
        id: badge
        anchors { right: parent.right; verticalCenter: parent.verticalCenter }
        width: 30
        horizontalAlignment: Text.AlignRight
        text: "CLIP"
        color: root.clip ? Theme.urgent : "transparent"
        font.family: Theme.font
        font.pixelSize: 10
        font.bold: true
    }
    Rectangle {
        id: track
        anchors { left: caption.right; right: badge.left; rightMargin: 8; verticalCenter: parent.verticalCenter }
        height: 6
        radius: 3
        color: Theme.surface
        Rectangle {
            width: parent.width * root.fraction
            height: parent.height
            radius: 3
            color: root.fraction > 0.9 ? Theme.urgent : Theme.accent
            Behavior on width { NumberAnimation { duration: 80 } }
        }
        Rectangle {
            x: Math.max(0, parent.width * root.peak - 2)
            width: 2
            height: parent.height
            color: Theme.foreground
            visible: root.peak > 0.02
        }
    }
}
