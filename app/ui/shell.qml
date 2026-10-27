//@ pragma UseQApplication
import QtQuick
import QtQuick.Layouts
import Quickshell

// Janela do MicBridge para tiling WMs. O motor é o daemon `micbridge --quickshell`;
// fechar a janela encerra só este processo, o microfone continua na bandeja.
ShellRoot {
    Backend { id: backend }

    FloatingWindow {
        id: win
        title: "MicBridge"
        implicitWidth: 820
        implicitHeight: 560
        color: Theme.background
        onVisibleChanged: if (!visible) Qt.quit()

        readonly property var st: backend.state
        readonly property string link: st.link || "disconnected"
        readonly property color linkColor: link === "connected" ? Theme.ok
            : link === "error" ? Theme.urgent
            : (link === "connecting" || link === "reconnecting") ? Theme.warn : Theme.muted

        Item {
            anchors.fill: parent
            focus: true
            Keys.onPressed: function(e) {
                var profiles = win.st.profiles || []
                if (e.key === Qt.Key_Escape || (e.key === Qt.Key_Q && e.modifiers & Qt.ControlModifier)) Qt.quit()
                else if (e.key === Qt.Key_M) backend.send({ c: "mute", on: !win.st.muted })
                else if (e.key === Qt.Key_P) backend.send({ c: "pairing", on: !(win.st.pairing > 0) })
                else if (e.key >= Qt.Key_1 && e.key <= Qt.Key_9 && profiles[e.key - Qt.Key_1])
                    backend.send({ c: "profile", id: profiles[e.key - Qt.Key_1].id })
                else return
                e.accepted = true
            }

            // Sem daemon: um aviso só, no lugar da interface inteira.
            Text {
                anchors.centerIn: parent
                visible: !backend.connected
                text: "Aguardando o MicBridge…"
                color: Theme.muted
                font.family: Theme.font
                font.pixelSize: 14
            }

            RowLayout {
                anchors.fill: parent
                spacing: 0
                visible: backend.connected

                // ── Barra lateral: estado e conexão ──
                Rectangle {
                    Layout.fillHeight: true
                    Layout.preferredWidth: 300
                    color: Qt.darker(Theme.background, 1.15)

                    ColumnLayout {
                        anchors.fill: parent
                        anchors.margins: 18
                        spacing: Theme.gap

                        RowLayout {
                            spacing: 10
                            Rectangle {
                                width: 10; height: 10; radius: 5
                                color: win.linkColor
                                SequentialAnimation on opacity {
                                    running: win.link === "connecting" || win.link === "reconnecting"
                                    loops: Animation.Infinite
                                    NumberAnimation { to: 0.2; duration: 500 }
                                    NumberAnimation { to: 1; duration: 500 }
                                }
                            }
                            Text { text: win.st.label || ""; color: Theme.foreground; font.family: Theme.font; font.pixelSize: 16; font.bold: true }
                        }
                        Text {
                            Layout.fillWidth: true
                            text: win.st.detail || ""
                            color: Theme.muted
                            font.family: Theme.font
                            font.pixelSize: 12
                            wrapMode: Text.Wrap
                        }

                        Section { text: "Conexão" }
                        QrCode {
                            Layout.alignment: Qt.AlignHCenter
                            Layout.preferredWidth: 200
                            Layout.preferredHeight: 200
                            rows: backend.qrRows
                            opacity: win.st.pairing > 0 ? 1 : 0.25
                        }
                        Text {
                            Layout.fillWidth: true
                            horizontalAlignment: Text.AlignHCenter
                            text: win.st.pairing > 0
                                ? "Escaneie para parear (" + Math.floor(win.st.pairing / 60) + ":" + String(win.st.pairing % 60).padStart(2, "0") + ")"
                                : "Celular pareado: leia o QR para abrir"
                            color: Theme.muted
                            font.family: Theme.font
                            font.pixelSize: 11
                            wrapMode: Text.Wrap
                        }
                        Text {
                            Layout.fillWidth: true
                            horizontalAlignment: Text.AlignHCenter
                            text: backend.url
                            color: Theme.accent
                            font.family: Theme.font
                            font.pixelSize: 11
                            elide: Text.ElideMiddle
                        }
                        Toggle {
                            Layout.fillWidth: true
                            title: "Aceitar novo celular"
                            hint: (win.st.devices || 0) + " pareado(s)  ·  P"
                            checked: win.st.pairing > 0
                            onToggled: on => backend.send({ c: "pairing", on: on })
                        }
                        Item { Layout.fillHeight: true }
                    }
                }

                // ── Painel principal: voz ──
                Flickable {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    contentHeight: main.implicitHeight + 36
                    clip: true

                    ColumnLayout {
                        id: main
                        x: 18; y: 18
                        width: Math.min(parent.width - 36, 640)
                        spacing: 6

                        Section { text: "Saída" }
                        Meter { Layout.fillWidth: true; label: "entrada"; level: win.st.in || 0 }
                        Meter { Layout.fillWidth: true; label: "saída"; level: win.st.out || 0 }
                        Toggle {
                            Layout.fillWidth: true
                            title: "Silenciar microfone virtual"
                            hint: "Retorno e gravação seguem ativos  ·  M"
                            checked: win.st.muted || false
                            onToggled: on => backend.send({ c: "mute", on: on })
                        }

                        Section { text: "Perfil de voz" }
                        Flow {
                            Layout.fillWidth: true
                            spacing: 6
                            Repeater {
                                model: win.st.profiles || []
                                delegate: Rectangle {
                                    required property var modelData
                                    required property int index
                                    readonly property bool active: modelData.id === win.st.profile
                                    width: chip.implicitWidth + 24
                                    height: 30
                                    radius: Theme.radius
                                    color: active ? Theme.accent : (chipArea.containsMouse ? Theme.surface : "transparent")
                                    border.color: active ? Theme.accent : Theme.surface
                                    Text {
                                        id: chip
                                        anchors.centerIn: parent
                                        text: (index < 9 ? (index + 1) + "  " : "") + modelData.name
                                        color: parent.active ? Theme.background : Theme.foreground
                                        font.family: Theme.font
                                        font.pixelSize: 12
                                    }
                                    MouseArea {
                                        id: chipArea
                                        anchors.fill: parent
                                        hoverEnabled: true
                                        cursorShape: Qt.PointingHandCursor
                                        onClicked: backend.send({ c: "profile", id: modelData.id })
                                    }
                                }
                            }
                        }
                        Text {
                            Layout.fillWidth: true
                            text: {
                                var list = win.st.profiles || []
                                for (var i = 0; i < list.length; i++) if (list[i].id === win.st.profile) return list[i].description
                                return ""
                            }
                            color: Theme.muted
                            font.family: Theme.font
                            font.pixelSize: 11
                        }

                        Section { text: "Tratamento" }
                        Repeater {
                            model: [
                                { key: "denoise", title: "Redução de ruído", hint: "RNNoise" },
                                { key: "highpass", title: "Filtro de graves", hint: "Corta rumble abaixo de ~80 Hz" },
                                { key: "gate", title: "Gate", hint: "Fecha o microfone no silêncio" },
                                { key: "deesser", title: "De-esser", hint: "Suaviza sibilantes" },
                                { key: "presence", title: "Presença", hint: "Realça a inteligibilidade (~3 kHz)" },
                                { key: "compressor", title: "Compressor", hint: "Nivela picos de volume" },
                                { key: "auto_gain", title: "Volume automático", hint: "Mantém o volume constante" }
                            ]
                            delegate: Toggle {
                                required property var modelData
                                Layout.fillWidth: true
                                title: modelData.title
                                hint: modelData.hint
                                checked: win.st.effects ? win.st.effects[modelData.key] : false
                                onToggled: on => backend.send({ c: "effect", name: modelData.key, on: on })
                            }
                        }
                    }
                }
            }
        }
    }
}
