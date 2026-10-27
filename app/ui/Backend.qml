import QtQuick
import Quickshell
import Quickshell.Io

// Ponte com o daemon: um JSON por linha no socket que o micbridge abriu.
Item {
    id: root

    property bool connected: socket.connected
    property var state: ({})
    property string url: ""
    property var qrRows: []

    function send(object) {
        if (!socket.connected) return
        socket.write(JSON.stringify(object) + "\n")
        socket.flush()
    }

    Socket {
        id: socket
        path: Quickshell.env("MICBRIDGE_SOCKET") || (Quickshell.env("XDG_RUNTIME_DIR") + "/micbridge.sock")
        connected: true
        parser: SplitParser {
            onRead: function(line) {
                var msg
                try { msg = JSON.parse(line) } catch (e) { return }
                if (msg.t === "state") root.state = msg
                else if (msg.t === "qr") { root.url = msg.url; root.qrRows = msg.rows }
            }
        }
        // O daemon pode estar subindo: tenta de novo até conectar.
        onConnectedChanged: if (!connected) retry.start()
    }
    Timer {
        id: retry
        interval: 1000
        onTriggered: socket.connected = true
    }
}
