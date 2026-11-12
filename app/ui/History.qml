import QtQuick

// Histórico rolante de entrada e saída em dBFS (-60 a 0): mostra o gate zerando
// o silêncio, o compressor achatando picos e o volume automático subindo a voz.
Item {
    id: root
    property real input: 0
    property real output: 0
    property int seconds: 20
    readonly property int rate: 20
    readonly property int capacity: seconds * rate
    property var inputs: []
    property var outputs: []

    function fraction(level) {
        return level <= 0 ? 0 : Math.max(0, Math.min(1, (20 * Math.log(level) / Math.LN10 + 60) / 60))
    }

    // Amostra em ritmo fixo, independente de quantas vezes o estado chega.
    Timer {
        interval: 1000 / root.rate
        running: root.visible
        repeat: true
        onTriggered: {
            root.inputs.push(root.fraction(root.input))
            root.outputs.push(root.fraction(root.output))
            if (root.inputs.length > root.capacity) { root.inputs.shift(); root.outputs.shift() }
            canvas.requestPaint()
        }
    }

    Row {
        id: legend
        spacing: 14
        Repeater {
            model: [{ label: I18n.t("meter.in"), color: Theme.muted }, { label: I18n.t("meter.out"), color: Theme.accent }]
            delegate: Row {
                required property var modelData
                spacing: 6
                Rectangle { width: 10; height: 3; radius: 1; color: modelData.color; anchors.verticalCenter: parent.verticalCenter }
                Text { text: modelData.label; color: Theme.muted; font.family: Theme.font; font.pixelSize: 11 }
            }
        }
    }
    Text {
        anchors.right: parent.right
        text: I18n.t("history.last", { n: root.seconds })
        color: Theme.muted
        font.family: Theme.font
        font.pixelSize: 11
    }

    Rectangle {
        anchors { left: parent.left; right: parent.right; top: legend.bottom; bottom: parent.bottom; topMargin: 8 }
        radius: Theme.radius
        color: Theme.surface

        Canvas {
            id: canvas
            anchors.fill: parent
            anchors.margins: 8

            onPaint: {
                var ctx = getContext("2d")
                ctx.clearRect(0, 0, width, height)
                ctx.font = "10px '" + Theme.font + "'"

                // Linhas de referência a -20 e -40 dB.
                ctx.strokeStyle = Qt.rgba(Theme.muted.r, Theme.muted.g, Theme.muted.b, 0.25)
                ctx.fillStyle = Theme.muted
                ctx.lineWidth = 1
                for (var db = -20; db >= -40; db -= 20) {
                    var gy = Math.round(height * (-db / 60)) + 0.5
                    ctx.beginPath(); ctx.moveTo(0, gy); ctx.lineTo(width, gy); ctx.stroke()
                    ctx.fillText(db + " dB", 2, gy - 3)
                }

                var n = root.inputs.length
                if (n < 2) return
                var step = width / (root.capacity - 1)
                var x0 = width - (n - 1) * step

                // Entrada como área preenchida, saída como linha por cima.
                ctx.beginPath()
                ctx.moveTo(x0, height)
                for (var i = 0; i < n; i++) ctx.lineTo(x0 + i * step, height * (1 - root.inputs[i]))
                ctx.lineTo(width, height)
                ctx.closePath()
                ctx.fillStyle = Qt.rgba(Theme.muted.r, Theme.muted.g, Theme.muted.b, 0.35)
                ctx.fill()

                ctx.beginPath()
                for (var j = 0; j < n; j++) {
                    var y = height * (1 - root.outputs[j])
                    if (j === 0) ctx.moveTo(x0, y); else ctx.lineTo(x0 + j * step, y)
                }
                ctx.strokeStyle = Theme.accent
                ctx.lineWidth = 2
                ctx.lineJoin = "round"
                ctx.stroke()
            }
        }
    }
}
