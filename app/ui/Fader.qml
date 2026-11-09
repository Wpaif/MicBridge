import QtQuick

// Fader vertical de mesa de som. Mesmo comportamento do Slider: durante o arraste
// mostra o valor local; fora dele, o do daemon. Ignora a roda do mouse para a
// rolagem da página não mexer no áudio.
Item {
    id: root
    property string label: ""
    property real from: 0
    property real to: 1
    property real step: 1
    property real value: 0
    property string unit: ""
    property bool enabled: true
    // Valor de repouso: o duplo clique volta para ele (NaN = sem repouso).
    property real home: NaN
    // Mostra "+" nos positivos, para ganhos que vão para os dois lados.
    property bool signed: false
    signal moved(real value)

    property real _local: value
    readonly property real shown: drag.pressed || settle.running ? _local : value
    onValueChanged: if (Math.abs(value - _local) < step / 2) settle.stop()
    readonly property real fraction: Math.max(0, Math.min(1, (shown - from) / (to - from)))

    implicitWidth: 40
    implicitHeight: 220
    opacity: enabled ? 1 : 0.35

    // Teclado: ↑/↓ um passo, PgUp/PgDn dez, Home/End nos extremos.
    activeFocusOnTab: root.enabled && root.visible
    function nudge(v) {
        v = Math.max(from, Math.min(to, v))
        v = Math.round(v / step) * step
        if (v === root.shown) return
        root._local = v
        root.moved(v)
        settle.restart()
    }
    Keys.onPressed: e => {
        var base = root.shown
        if (e.key === Qt.Key_Up || e.key === Qt.Key_Right) nudge(base + step)
        else if (e.key === Qt.Key_Down || e.key === Qt.Key_Left) nudge(base - step)
        else if (e.key === Qt.Key_PageUp) nudge(base + 10 * step)
        else if (e.key === Qt.Key_PageDown) nudge(base - 10 * step)
        else if (e.key === Qt.Key_Home) nudge(from)
        else if (e.key === Qt.Key_End) nudge(to)
        else return
        e.accepted = true
    }
    Timer { id: settle; interval: 600 }

    function text(v) {
        var t = (step < 1 ? v.toFixed(1) : v.toFixed(0)).replace(".", ",")
        return (root.signed && v > 0 ? "+" : "") + t + root.unit
    }

    Text {
        id: readout
        anchors { top: parent.top; horizontalCenter: parent.horizontalCenter }
        text: root.text(root.shown)
        color: drag.pressed || root.activeFocus ? Theme.accent : Theme.foreground
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

    Item {
        id: slot
        anchors { top: readout.bottom; bottom: caption.top; topMargin: 10; bottomMargin: 10; horizontalCenter: parent.horizontalCenter }
        width: 28

        // Marcas da escala, como numa mesa.
        Repeater {
            model: 11
            Rectangle {
                required property int index
                x: index % 5 === 0 ? 0 : 5
                width: index % 5 === 0 ? slot.width : slot.width - 10
                height: 1
                y: Math.round(index * (slot.height - 1) / 10)
                color: Theme.surface
            }
        }
        // Trilho.
        Rectangle {
            id: track
            anchors { top: parent.top; bottom: parent.bottom; horizontalCenter: parent.horizontalCenter }
            width: 4
            radius: 2
            color: Qt.darker(Theme.surface, 1.3)
            // Faixas com sinal (EQ) acendem a partir do zero; as outras, de baixo.
            readonly property real base: root.signed ? (0 - root.from) / (root.to - root.from) : 0
            Rectangle {
                width: parent.width
                y: parent.height * (1 - Math.max(root.fraction, track.base))
                height: parent.height * Math.abs(root.fraction - track.base)
                radius: 2
                color: Theme.accent
                opacity: 0.7
            }
        }
        // Capa do fader.
        Rectangle {
            id: cap
            width: slot.width
            height: 14
            radius: 3
            y: (slot.height - height) * (1 - root.fraction)
            color: drag.pressed || drag.containsMouse || root.activeFocus ? Theme.accent : Theme.foreground
            border.color: Theme.background
            Rectangle {
                anchors.centerIn: parent
                width: parent.width - 8; height: 2
                color: Theme.background
                opacity: 0.6
            }
        }
        MouseArea {
            id: drag
            anchors.fill: parent
            anchors.margins: -8
            enabled: root.enabled
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            preventStealing: true
            onWheel: wheel => wheel.accepted = false
            function update(my) {
                var f = 1 - Math.max(0, Math.min(1, (my - 8 - cap.height / 2) / (slot.height - cap.height)))
                var v = Math.round((root.from + f * (root.to - root.from)) / root.step) * root.step
                if (v !== root._local) { root._local = v; root.moved(v) }
            }
            onPressed: mouse => { root._local = root.value; update(mouse.y) }
            onPositionChanged: mouse => { if (pressed) update(mouse.y) }
            onReleased: settle.restart()
            // Duplo clique volta ao repouso (se houver) e foca para ajuste fino pelo teclado.
            onDoubleClicked: {
                if (!isNaN(root.home)) root.nudge(root.home)
                root.forceActiveFocus()
            }
        }
    }

    Rectangle {
        anchors.fill: parent
        anchors.margins: -4
        radius: Theme.radius
        color: "transparent"
        border.color: Theme.accent
        visible: root.activeFocus
    }
}
