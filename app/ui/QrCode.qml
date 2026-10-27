import QtQuick

// Desenha a matriz que o daemon manda (linhas de '0'/'1') com zona de silêncio.
Rectangle {
    id: root
    property var rows: []
    color: "white"
    radius: Theme.radius
    onRowsChanged: canvas.requestPaint()

    Canvas {
        id: canvas
        anchors.fill: parent
        anchors.margins: 10
        onPaint: {
            var ctx = getContext("2d")
            ctx.reset()
            var n = root.rows.length
            if (n === 0) return
            var cell = Math.floor(Math.min(width, height) / n)
            var off = Math.floor((Math.min(width, height) - cell * n) / 2)
            ctx.fillStyle = "black"
            for (var y = 0; y < n; y++)
                for (var x = 0; x < n; x++)
                    if (root.rows[y].charAt(x) === "1") ctx.fillRect(off + x * cell, off + y * cell, cell, cell)
        }
    }
}
