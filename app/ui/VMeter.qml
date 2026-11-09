import QtQuick

// Medidor vertical. `fraction` de 0 a 1; `fromTop` faz a barra descer do topo,
// como o medidor de redução de ganho (GR) dos compressores. `center` desenha a
// partir do meio, para valores com sinal (ajuste de ganho para mais ou para menos).
Item {
    id: root
    property real fraction: 0
    property bool fromTop: false
    property bool center: false
    property color color: Theme.accent
    property string label: ""
    property string readout: ""
    property bool live: true
    // Saturação recente: o valor no topo vira "CLIP" e a barra fica vermelha.
    property bool clip: false

    implicitWidth: 24
    implicitHeight: 220

    Text {
        id: top
        anchors { top: parent.top; horizontalCenter: parent.horizontalCenter }
        text: root.clip ? "CLIP" : root.live ? root.readout : "—"
        color: root.clip ? Theme.urgent : root.live ? Theme.foreground : Theme.muted
        font.bold: root.clip
        font.family: Theme.font
        font.pixelSize: 11
    }
    Text {
        id: caption
        anchors { bottom: parent.bottom; horizontalCenter: parent.horizontalCenter }
        text: root.label
        color: Theme.muted
        font.family: Theme.font
        font.pixelSize: 10
    }
    Rectangle {
        id: well
        anchors { top: top.bottom; bottom: caption.top; topMargin: 10; bottomMargin: 10; horizontalCenter: parent.horizontalCenter }
        width: 8
        radius: 2
        color: Qt.darker(Theme.surface, 1.3)

        readonly property real f: Math.max(-1, Math.min(1, root.live ? root.fraction : 0))
        Rectangle {
            width: parent.width
            radius: 2
            color: root.clip ? Theme.urgent : root.color
            height: root.center ? parent.height / 2 * Math.abs(well.f) : parent.height * Math.max(0, well.f)
            y: root.center ? (well.f >= 0 ? parent.height / 2 - height : parent.height / 2)
                           : (root.fromTop ? 0 : parent.height - height)
            Behavior on height { NumberAnimation { duration: 90 } }
        }
        Rectangle {
            visible: root.center
            width: parent.width + 6; height: 1
            x: -3; y: parent.height / 2
            color: Theme.muted
        }
    }
}
