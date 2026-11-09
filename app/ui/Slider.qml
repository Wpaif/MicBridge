import QtQuick

// Ajuste numérico. Durante o arraste mostra o valor local; fora dele, o do daemon.
// Não reage à roda do mouse, para a rolagem do painel não mexer no áudio.
Item {
    id: root
    property string title: ""
    property real from: 0
    property real to: 1
    property real step: 1
    property real value: 0
    property string unit: ""
    property bool enabled: true
    signal moved(real value)

    property real _local: value
    // Depois de soltar, segura o valor local até o daemon confirmar (o estado chega a cada 100 ms).
    readonly property real shown: drag.pressed || settle.running ? _local : value
    onValueChanged: if (Math.abs(value - _local) < step / 2) settle.stop()
    readonly property real fraction: (shown - from) / (to - from)

    implicitHeight: 40
    opacity: enabled ? 1 : 0.4

    // Teclado: ←/→ um passo, PgUp/PgDn dez, Home/End nos extremos.
    activeFocusOnTab: root.enabled && root.visible
    function nudge(to) {
        var v = Math.max(from, Math.min(to, root.to))
        v = Math.round(v / step) * step
        if (v === root.shown) return
        root._local = v
        root.moved(v)
        settle.restart()
    }
    Keys.onPressed: e => {
        var base = root.shown
        if (e.key === Qt.Key_Left) nudge(base - step)
        else if (e.key === Qt.Key_Right) nudge(base + step)
        else if (e.key === Qt.Key_PageDown) nudge(base - 10 * step)
        else if (e.key === Qt.Key_PageUp) nudge(base + 10 * step)
        else if (e.key === Qt.Key_Home) nudge(from)
        else if (e.key === Qt.Key_End) nudge(to)
        else return
        e.accepted = true
    }
    Rectangle {
        anchors.fill: parent
        radius: Theme.radius
        color: "transparent"
        border.color: Theme.accent
        visible: root.activeFocus
    }

    function snap(x) {
        var raw = from + Math.max(0, Math.min(1, x / track.width)) * (to - from)
        return Math.round(raw / step) * step
    }
    function label(v) {
        return (step < 1 ? v.toFixed(1) : v.toFixed(0)).replace(".", ",") + root.unit
    }

    Timer { id: settle; interval: 600 }

    Text {
        anchors { left: parent.left; leftMargin: 10; verticalCenter: parent.verticalCenter }
        width: 170
        text: root.title
        color: Theme.foreground
        font.family: Theme.font
        font.pixelSize: 12
        elide: Text.ElideRight
    }
    Text {
        id: readout
        anchors { right: parent.right; rightMargin: 10; verticalCenter: parent.verticalCenter }
        width: 64
        horizontalAlignment: Text.AlignRight
        text: root.label(root.shown)
        color: Theme.muted
        font.family: Theme.font
        font.pixelSize: 12
    }
    Rectangle {
        id: track
        anchors { left: parent.left; leftMargin: 190; right: readout.left; rightMargin: 12; verticalCenter: parent.verticalCenter }
        height: 4
        radius: 2
        color: Theme.surface
        Rectangle {
            width: parent.width * root.fraction
            height: parent.height
            radius: 2
            color: Theme.accent
        }
        Rectangle {
            width: 14; height: 14; radius: 7
            x: parent.width * root.fraction - width / 2
            anchors.verticalCenter: parent.verticalCenter
            color: drag.pressed || drag.containsMouse || root.activeFocus ? Theme.accent : Theme.foreground
        }
        MouseArea {
            id: drag
            anchors.fill: parent
            anchors.margins: -10
            enabled: root.enabled
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            preventStealing: true
            onWheel: wheel => wheel.accepted = false
            function update(mx) {
                var v = root.snap(mx - 10)
                if (v !== root._local) { root._local = v; root.moved(v) }
            }
            onPressed: mouse => { root._local = root.value; update(mouse.x) }
            onPositionChanged: mouse => { if (pressed) update(mouse.x) }
            onReleased: settle.restart()
        }
    }
}
