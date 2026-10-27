//@ pragma UseQApplication
//@ pragma AppId dev.wilian.MicBridge
import QtQuick
import QtQuick.Layouts
import QtQuick.Dialogs
import Quickshell

// Janela do MicBridge para tiling WMs. O motor é o daemon `micbridge --quickshell`;
// fechar a janela encerra só este processo, o microfone continua na bandeja.
ShellRoot {
    Backend { id: backend }

    // O par -tratado.wav sai ao lado do arquivo escolhido; a substituição é confirmada na janela.
    FileDialog {
        id: exportDialog
        title: "Salvar teste original e tratado"
        fileMode: FileDialog.SaveFile
        acceptLabel: "Exportar"
        nameFilters: ["Áudio WAV (*.wav)"]
        currentFolder: "file://" + Quickshell.env("HOME")
        selectedFile: "file://" + Quickshell.env("HOME") + "/teste-original.wav"
        options: FileDialog.DontConfirmOverwrite
        onAccepted: backend.send({ c: "export", path: decodeURIComponent(String(selectedFile).replace(/^file:\/\//, "")) })
    }

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

        readonly property var mon: st.monitor || ({})
        readonly property var rec: st.recording || ({})
        readonly property var editor: st.editor || ({})
        // "", "new", "rename" ou "delete": qual edição de perfil está aberta.
        property string profileEdit: ""
        // Perfil escolhido com alterações não salvas, aguardando confirmação.
        property string pendingProfile: ""
        property bool forgetting: false
        property bool copied: false

        function chooseProfile(id) {
            if (id === st.profile || !editor.can_select) return
            if (editor.dirty) pendingProfile = id
            else backend.send({ c: "profile", id: id })
        }
        function editProfile(mode) {
            var name = ""
            if (mode === "rename") {
                var list = st.profiles || []
                for (var i = 0; i < list.length; i++) if (list[i].id === st.profile) name = list[i].name
            }
            nameField.text = name
            profileEdit = mode
            nameField.focusInput()
        }
        function effect(key) { return st.effects ? st.effects[key] === true : false }
        function setEffect(key, on) { backend.send({ c: "effect", name: key, on: on }) }
        function tuning(key) { return st.tuning ? st.tuning[key] : 0 }
        function activity(key) { return st.activity ? st.activity[key] : "" }

        // Fechar o campo de nome devolve o teclado aos atalhos.
        onProfileEditChanged: if (profileEdit === "") shortcuts.forceActiveFocus()

        Item {
            id: shortcuts
            anchors.fill: parent
            focus: true
            Keys.onPressed: function(e) {
                var profiles = win.st.profiles || []
                // Esc fecha primeiro o que estiver aberto; só depois a janela.
                if (e.key === Qt.Key_Escape && win.forgetting) win.forgetting = false
                else if (e.key === Qt.Key_Escape && win.pendingProfile !== "") win.pendingProfile = ""
                else if (e.key === Qt.Key_Escape && win.profileEdit !== "") win.profileEdit = ""
                else if (e.key === Qt.Key_Escape && win.rec.confirm) backend.send({ c: "export_confirm", on: false })
                else if (e.key === Qt.Key_Escape || (e.key === Qt.Key_Q && e.modifiers & Qt.ControlModifier)) Qt.quit()
                else if (e.key === Qt.Key_M) backend.send({ c: "mute", on: !win.st.muted })
                else if (e.key === Qt.Key_N && win.editor.can_new) win.editProfile("new")
                else if (e.key === Qt.Key_S && win.editor.can_save) backend.send({ c: "profile_save" })
                else if (e.key === Qt.Key_R && win.editor.can_rename) win.editProfile("rename")
                else if (e.key === Qt.Key_Delete && win.editor.can_delete) win.profileEdit = "delete"
                else if (e.key === Qt.Key_P) backend.send({ c: "pairing", on: !(win.st.pairing > 0) })
                else if (e.key >= Qt.Key_1 && e.key <= Qt.Key_9 && profiles[e.key - Qt.Key_1])
                    win.chooseProfile(profiles[e.key - Qt.Key_1].id)
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
                            hint: "Abre o QR por 5 minutos  ·  P"
                            checked: win.st.pairing > 0
                            onToggled: on => backend.send({ c: "pairing", on: on })
                        }
                        RowLayout {
                            Layout.fillWidth: true
                            Layout.leftMargin: 10
                            visible: (win.st.devices || 0) > 0 && !win.forgetting
                            Text {
                                Layout.fillWidth: true
                                Layout.minimumWidth: 0
                                text: win.st.devices === 1 ? "1 celular pareado" : win.st.devices + " celulares pareados"
                                color: Theme.foreground
                                font.family: Theme.font
                                font.pixelSize: 13
                                elide: Text.ElideRight
                            }
                            Button { text: "Esquecer"; onClicked: win.forgetting = true }
                        }
                        Confirm {
                            Layout.fillWidth: true
                            visible: win.forgetting
                            title: "Esquecer todos os celulares?"
                            body: "Eles precisarão ler o QR e parear de novo."
                            action: "Esquecer"
                            onAccepted: { backend.send({ c: "forget" }); win.forgetting = false }
                            onRejected: win.forgetting = false
                        }
                        Toggle {
                            Layout.fillWidth: true
                            title: "Iniciar com o sistema"
                            hint: "Abre escondido na bandeja"
                            checked: win.st.autostart || false
                            onToggled: on => backend.send({ c: "autostart", on: on })
                        }
                        Item { Layout.fillHeight: true }
                        // Confira no celular ao aceitar o aviso do certificado.
                        Section { text: "Certificado · SHA-256" }
                        RowLayout {
                            Layout.fillWidth: true
                            spacing: 6
                            Text {
                                Layout.fillWidth: true
                                text: win.st.fingerprint || "aguardando o servidor…"
                                color: Theme.muted
                                font.family: Theme.font
                                font.pixelSize: 10
                                wrapMode: Text.WrapAnywhere
                            }
                            Button {
                                text: win.copied ? "Copiado" : "Copiar"
                                enabled: !!win.st.fingerprint
                                onClicked: { Quickshell.clipboardText = win.st.fingerprint; win.copied = true; copiedTimer.restart() }
                                Timer { id: copiedTimer; interval: 1500; onTriggered: win.copied = false }
                            }
                        }
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

                        Section { text: "Retorno no fone" }
                        Toggle {
                            Layout.fillWidth: true
                            title: "Ouvir a voz tratada"
                            hint: win.mon.message || ""
                            checked: win.mon.active || false
                            enabled: win.mon.can_toggle || false
                            onToggled: on => backend.send({ c: "monitor", on: on })
                        }
                        Flow {
                            Layout.fillWidth: true
                            Layout.leftMargin: 10
                            spacing: 6
                            Repeater {
                                model: win.mon.outputs || []
                                delegate: Chip {
                                    required property var modelData
                                    text: modelData.label
                                    active: modelData.id === win.mon.selected
                                    enabled: win.mon.can_choose || false
                                    onClicked: backend.send({ c: "monitor_output", id: active ? null : modelData.id })
                                }
                            }
                            Button {
                                text: "Atualizar saídas"
                                enabled: win.mon.can_refresh || false
                                onClicked: backend.send({ c: "monitor_refresh" })
                            }
                            Button {
                                text: "Restaurar conexões"
                                accent: true
                                visible: win.mon.can_retry || false
                                onClicked: backend.send({ c: "monitor_retry" })
                            }
                        }
                        Slider {
                            Layout.fillWidth: true
                            title: "Volume do retorno"
                            from: 0; to: 100; step: 1; unit: "%"
                            value: win.mon.volume || 0
                            onMoved: v => backend.send({ c: "monitor_volume", value: v })
                        }
                        Text {
                            Layout.fillWidth: true
                            Layout.leftMargin: 10
                            text: "Exclusivo: desconecta outros apps da saída escolhida e os restaura ao parar."
                            color: Theme.muted
                            font.family: Theme.font
                            font.pixelSize: 11
                            wrapMode: Text.Wrap
                        }

                        Section { text: "Teste de voz" }
                        RowLayout {
                            Layout.fillWidth: true
                            Layout.leftMargin: 10
                            Layout.rightMargin: 10
                            spacing: 10
                            Text {
                                text: win.rec.caption || ""
                                color: Theme.foreground
                                font.family: Theme.font
                                font.pixelSize: 12
                            }
                            Rectangle {
                                Layout.fillWidth: true
                                height: 4
                                radius: 2
                                color: Theme.surface
                                Rectangle {
                                    width: parent.width * (win.rec.progress || 0)
                                    height: parent.height
                                    radius: 2
                                    color: win.rec.can_stop ? Theme.urgent : Theme.accent
                                }
                            }
                        }
                        Flow {
                            Layout.fillWidth: true
                            Layout.leftMargin: 10
                            spacing: 6
                            Button {
                                text: win.rec.can_stop ? "Parar gravação" : "Gravar teste"
                                accent: win.rec.can_stop || false
                                enabled: win.rec.can_start || win.rec.can_stop || false
                                onClicked: backend.send({ c: "record", on: !win.rec.can_stop })
                            }
                            Chip {
                                text: "Ouvir original"
                                active: win.rec.playing === "original"
                                enabled: win.rec.can_listen || false
                                onClicked: backend.send(active ? { c: "stop_play" } : { c: "play", version: "original" })
                            }
                            Chip {
                                text: "Ouvir tratado"
                                active: win.rec.playing === "treated"
                                enabled: win.rec.can_listen || false
                                onClicked: backend.send(active ? { c: "stop_play" } : { c: "play", version: "treated" })
                            }
                            Button {
                                text: "Exportar WAVs"
                                enabled: (win.rec.can_export || false) && !win.rec.confirm
                                onClicked: exportDialog.open()
                            }
                        }
                        Text {
                            Layout.fillWidth: true
                            Layout.leftMargin: 10
                            text: win.rec.message || ""
                            color: Theme.muted
                            font.family: Theme.font
                            font.pixelSize: 11
                            wrapMode: Text.Wrap
                        }
                        Confirm {
                            Layout.fillWidth: true
                            visible: !!win.rec.confirm
                            title: "Substituir arquivos do teste?"
                            body: win.rec.confirm ? "Os que já existem serão substituídos.\n" + win.rec.confirm.original + "\n" + win.rec.confirm.treated : ""
                            action: "Substituir"
                            onAccepted: backend.send({ c: "export_confirm", on: true })
                            onRejected: backend.send({ c: "export_confirm", on: false })
                        }

                        Section { text: "Perfil de voz" }
                        Flow {
                            Layout.fillWidth: true
                            spacing: 6
                            Repeater {
                                model: win.st.profiles || []
                                delegate: Chip {
                                    required property var modelData
                                    required property int index
                                    text: (index < 9 ? (index + 1) + "  " : "") + modelData.name
                                    active: modelData.id === win.st.profile
                                    enabled: win.editor.can_select || false
                                    onClicked: win.chooseProfile(modelData.id)
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
                        Flow {
                            Layout.fillWidth: true
                            spacing: 6
                            visible: win.profileEdit === ""
                            Button {
                                text: "Salvar alterações  ·  S"
                                accent: true
                                enabled: win.editor.can_save || false
                                onClicked: backend.send({ c: "profile_save" })
                            }
                            Button {
                                text: "Salvar como novo…  ·  N"
                                enabled: win.editor.can_new || false
                                onClicked: win.editProfile("new")
                            }
                            Button {
                                text: "Renomear…  ·  R"
                                enabled: win.editor.can_rename || false
                                onClicked: win.editProfile("rename")
                            }
                            Button {
                                text: "Excluir"
                                enabled: win.editor.can_delete || false
                                onClicked: win.profileEdit = "delete"
                            }
                        }
                        NameField {
                            id: nameField
                            Layout.fillWidth: true
                            visible: win.profileEdit === "new" || win.profileEdit === "rename"
                            action: win.profileEdit === "rename" ? "Renomear" : "Criar perfil"
                            onAccepted: name => {
                                backend.send({ c: win.profileEdit === "rename" ? "profile_rename" : "profile_new", name: name })
                                win.profileEdit = ""
                            }
                            onRejected: win.profileEdit = ""
                        }
                        Confirm {
                            Layout.fillWidth: true
                            visible: win.profileEdit === "delete"
                            title: "Excluir perfil personalizado?"
                            body: "O perfil e seus ajustes não salvos serão removidos. O perfil Chamada será ativado."
                            action: "Excluir"
                            onAccepted: { backend.send({ c: "profile_delete" }); win.profileEdit = "" }
                            onRejected: win.profileEdit = ""
                        }
                        Confirm {
                            Layout.fillWidth: true
                            visible: win.pendingProfile !== ""
                            title: "Descartar alterações não salvas?"
                            body: "O perfil salvo será preservado. Os ajustes atuais serão descartados ao trocar de perfil."
                            action: "Descartar"
                            onAccepted: { backend.send({ c: "profile", id: win.pendingProfile }); win.pendingProfile = "" }
                            onRejected: win.pendingProfile = ""
                        }
                        Text {
                            Layout.fillWidth: true
                            text: win.editor.message || ""
                            color: win.editor.dirty ? Theme.warn : Theme.muted
                            font.family: Theme.font
                            font.pixelSize: 11
                            wrapMode: Text.Wrap
                        }

                        Section { text: "Limpeza" }
                        Toggle {
                            Layout.fillWidth: true
                            title: "Redução de ruído"
                            hint: "RNNoise"
                            checked: win.effect("denoise")
                            onToggled: on => win.setEffect("denoise", on)
                        }
                        Toggle {
                            Layout.fillWidth: true
                            title: "Filtro de graves"
                            hint: "Corta rumble abaixo de ~80 Hz"
                            checked: win.effect("highpass")
                            onToggled: on => win.setEffect("highpass", on)
                        }
                        Toggle {
                            Layout.fillWidth: true
                            title: "Noise gate"
                            hint: win.activity("gate")
                            checked: win.effect("gate")
                            onToggled: on => win.setEffect("gate", on)
                        }
                        Slider {
                            Layout.fillWidth: true
                            title: "Limiar do gate"
                            from: -80; to: -20; step: 1; unit: " dB"
                            value: win.tuning("gate_db")
                            enabled: win.effect("gate")
                            onMoved: v => backend.send({ c: "tune", name: "gate_db", value: v })
                        }

                        Section { text: "Tom" }
                        Toggle {
                            Layout.fillWidth: true
                            title: "Suavizar chiados (\"s\")"
                            hint: "De-esser"
                            checked: win.effect("deesser")
                            onToggled: on => win.setEffect("deesser", on)
                        }
                        Toggle {
                            Layout.fillWidth: true
                            title: "Realçar clareza da voz"
                            hint: "Presença (~3 kHz)"
                            checked: win.effect("presence")
                            onToggled: on => win.setEffect("presence", on)
                        }

                        Section { text: "Dinâmica" }
                        Toggle {
                            Layout.fillWidth: true
                            title: "Compressor"
                            hint: win.activity("compressor")
                            checked: win.effect("compressor")
                            onToggled: on => win.setEffect("compressor", on)
                        }
                        Slider {
                            Layout.fillWidth: true
                            title: "Limiar"
                            from: -50; to: 0; step: 1; unit: " dB"
                            value: win.tuning("threshold_db")
                            enabled: win.effect("compressor")
                            onMoved: v => backend.send({ c: "tune", name: "threshold_db", value: v })
                        }
                        Slider {
                            Layout.fillWidth: true
                            title: "Razão"
                            from: 1; to: 10; step: 0.5; unit: ":1"
                            value: win.tuning("ratio")
                            enabled: win.effect("compressor")
                            onMoved: v => backend.send({ c: "tune", name: "ratio", value: v })
                        }

                        Section { text: "Volume" }
                        Toggle {
                            Layout.fillWidth: true
                            title: "Volume automático"
                            hint: win.activity("auto_gain")
                            checked: win.effect("auto_gain")
                            onToggled: on => win.setEffect("auto_gain", on)
                        }
                        // O ganho manual só vale com o volume automático desligado.
                        Slider {
                            Layout.fillWidth: true
                            title: "Ganho manual"
                            from: 0; to: 24; step: 1; unit: " dB"
                            value: win.tuning("makeup_db")
                            enabled: !win.effect("auto_gain")
                            onMoved: v => backend.send({ c: "tune", name: "makeup_db", value: v })
                        }
                    }
                }
            }
        }
    }
}
