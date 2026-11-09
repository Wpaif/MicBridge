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
        // Painel largo (tela cheia): seções com mais conteúdo se abrem em duas colunas.
        readonly property bool wide: pane.width >= 1000
        readonly property bool widePage: page === "voz" || page === "efeitos" || page === "conexao"
        // Seção aberta no painel (o trilho lateral troca).
        property string page: "voz"
        readonly property var pages: [
            { id: "voz", label: "Voz", glyph: "\u{f036c}" },
            { id: "perfil", label: "Perfil", glyph: "\u{f0004}" },
            { id: "efeitos", label: "Efeitos", glyph: "\u{f062e}" },
            { id: "conexao", label: "Conexão", glyph: "\u{f011c}" }
        ]
        function pageLabel() {
            for (var i = 0; i < pages.length; i++) if (pages[i].id === page) return pages[i].label
            return ""
        }
        property bool helpOpen: false
        function goPage(id) {
            page = id
            pane.contentY = 0
            // O controle focado pode ter sumido com a seção: o foco volta para os atalhos.
            shortcuts.forceActiveFocus()
        }
        function moveFocus(forward) {
            var next = (focused || shortcuts).nextItemInFocusChain(forward)
            // Sem controle focado, ↑ vai para o último e ↓ para o primeiro.
            if (next && next !== shortcuts) next.forceActiveFocus()
        }
        // Rola o painel até o controle focado pelo teclado.
        readonly property Item focused: shortcuts.Window.activeFocusItem
        onFocusedChanged: {
            var it = focused
            if (!it || it === shortcuts || !pane) return
            var p = it.mapToItem(pane.contentItem, 0, 0)
            if (p.y < pane.contentY + 8) pane.contentY = Math.max(0, p.y - 24)
            else if (p.y + it.height > pane.contentY + pane.height - 8)
                pane.contentY = Math.min(pane.contentHeight - pane.height, p.y + it.height - pane.height + 24)
        }
        function stepPage(d) {
            var i = 0
            for (; i < pages.length; i++) if (pages[i].id === page) break
            goPage(pages[(i + d + pages.length) % pages.length].id)
        }
        // Bolinha no trilho: o que está ligado sem precisar abrir a seção.
        function badge(id) {
            if (id === "voz") return rec.can_stop ? Theme.urgent : (mon.active || (st.extra || {}).linked) ? Theme.accent : "transparent"
            if (id === "perfil") return editor.dirty ? Theme.warn : "transparent"
            if (id === "conexao") return st.pairing > 0 ? Theme.warn : "transparent"
            return "transparent"
        }
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
                else if (e.key === Qt.Key_Escape && win.helpOpen) win.helpOpen = false
                // Com um controle focado, o Esc primeiro só solta o foco.
                else if (e.key === Qt.Key_Escape && win.focused !== shortcuts) shortcuts.forceActiveFocus()
                else if (e.key === Qt.Key_Escape || (e.key === Qt.Key_Q && e.modifiers & Qt.ControlModifier)) Qt.quit()
                else if (e.key === Qt.Key_Question || (e.key === Qt.Key_Slash && e.modifiers & Qt.ShiftModifier) || e.key === Qt.Key_F1) win.helpOpen = !win.helpOpen
                // Seções: Ctrl+Tab, Ctrl+PgUp/PgDn, Alt+↑/↓ e Alt+1…4.
                else if (e.key === Qt.Key_Tab && e.modifiers & Qt.ControlModifier) win.stepPage(1)
                else if (e.key === Qt.Key_Backtab && e.modifiers & Qt.ControlModifier) win.stepPage(-1)
                else if (e.key === Qt.Key_PageDown && e.modifiers & Qt.ControlModifier) win.stepPage(1)
                else if (e.key === Qt.Key_PageUp && e.modifiers & Qt.ControlModifier) win.stepPage(-1)
                else if (e.key === Qt.Key_Down && e.modifiers & Qt.AltModifier) win.stepPage(1)
                else if (e.key === Qt.Key_Up && e.modifiers & Qt.AltModifier) win.stepPage(-1)
                else if (e.modifiers & Qt.AltModifier && e.key >= Qt.Key_1 && e.key < Qt.Key_1 + win.pages.length)
                    win.goPage(win.pages[e.key - Qt.Key_1].id)
                // Setas andam entre os controles da seção (o slider fica com ←/→ para si).
                else if (e.key === Qt.Key_Down || e.key === Qt.Key_Right) win.moveFocus(true)
                else if (e.key === Qt.Key_Up || e.key === Qt.Key_Left) win.moveFocus(false)
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


            // Folha de atalhos (? ou F1), como a do flea.
            Rectangle {
                anchors.fill: parent
                z: 10
                visible: win.helpOpen
                color: Qt.rgba(0, 0, 0, 0.6)
                MouseArea { anchors.fill: parent; onClicked: win.helpOpen = false }
                Rectangle {
                    anchors.centerIn: parent
                    width: Math.min(parent.width - 48, 520)
                    height: helpList.implicitHeight + 40
                    radius: Theme.radius
                    color: Theme.background
                    border.color: Theme.surface
                    MouseArea { anchors.fill: parent }
                    Column {
                        id: helpList
                        x: 20; y: 20
                        width: parent.width - 40
                        spacing: 4
                        Text { text: "Atalhos"; color: Theme.foreground; font.family: Theme.font; font.pixelSize: 15; font.bold: true; bottomPadding: 6 }
                        Repeater {
                            model: [
                                ["Tab  ·  ↑ ↓", "Próximo / anterior controle"],
                                ["← →", "Ajustar slider, ou andar na linha"],
                                ["PgUp PgDn  ·  Home End", "Slider: 10 passos / extremos"],
                                ["Espaço  ·  Enter", "Ligar, desligar ou clicar"],
                                ["Ctrl+Tab  ·  Alt+↑ ↓", "Próxima / seção anterior"],
                                ["Alt+1 … 4", "Ir direto para a seção"],
                                ["M", "Silenciar microfone"],
                                ["1 … 9", "Escolher perfil"],
                                ["N  ·  S  ·  R  ·  Del", "Perfil: novo, salvar, renomear, excluir"],
                                ["P", "Aceitar novo celular"],
                                ["Esc", "Soltar o foco; de novo, fechar"],
                                ["?  ·  F1", "Esta folha"]
                            ]
                            delegate: Row {
                                required property var modelData
                                spacing: 12
                                Text { width: 190; text: modelData[0]; color: Theme.accent; font.family: Theme.font; font.pixelSize: 12 }
                                Text { text: modelData[1]; color: Theme.foreground; font.family: Theme.font; font.pixelSize: 12 }
                            }
                        }
                    }
                }
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

                // ── Trilho lateral, como no Flea: estado no topo e uma seção por vez ──
                Rectangle {
                    Layout.fillHeight: true
                    Layout.preferredWidth: 210
                    color: Qt.darker(Theme.background, 1.15)

                    ColumnLayout {
                        anchors.fill: parent
                        anchors.topMargin: 18
                        anchors.bottomMargin: 12
                        spacing: 0

                        ColumnLayout {
                            Layout.fillWidth: true
                            Layout.leftMargin: 18
                            Layout.rightMargin: 14
                            Layout.bottomMargin: 14
                            spacing: 6
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
                            Text { text: win.st.label || ""; color: Theme.foreground; font.family: Theme.font; font.pixelSize: 14; font.bold: true }
                        }
                        Text {
                            Layout.fillWidth: true
                            text: win.st.detail || ""
                            color: Theme.muted
                            font.family: Theme.font
                            font.pixelSize: 11
                            wrapMode: Text.Wrap
                        }

                        }

                        Repeater {
                            model: win.pages
                            delegate: RailRow {
                                required property var modelData
                                required property int index
                                Layout.fillWidth: true
                                glyph: modelData.glyph
                                label: modelData.label
                                current: win.page === modelData.id
                                badge: win.badge(modelData.id)
                                onClicked: win.goPage(modelData.id)
                            }
                        }

                        Item { Layout.fillHeight: true }

                        Text {
                            Layout.leftMargin: 18
                            Layout.bottomMargin: 6
                            text: "?  atalhos"
                            color: Theme.muted
                            font.family: Theme.font
                            font.pixelSize: 11
                            MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: win.helpOpen = true }
                        }

                        // Silenciar fica sempre à mão, em qualquer seção.
                        RailRow {
                            Layout.fillWidth: true
                            glyph: win.st.muted ? "\u{f036d}" : "\u{f036c}"
                            label: win.st.muted ? "Silenciado" : "Silenciar"
                            current: win.st.muted || false
                            alert: true
                            onClicked: backend.send({ c: "mute", on: !win.st.muted })
                        }
                    }
                }

                // ── Painel: só a seção escolhida ──
                Flickable {
                    id: pane
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    contentHeight: pageStack.implicitHeight + 36
                    clip: true
                    boundsBehavior: Flickable.StopAtBounds

                    ColumnLayout {
                        id: pageStack
                        // Formulários ficam numa coluna legível; o que tem gráfico ou duas colunas
                        // usa a largura da tela cheia.
                        width: Math.min(pane.width - 48, win.widePage ? 1200 : 720)
                        x: Math.max(24, (pane.width - width) / 2); y: win.wide ? 36 : 18
                        spacing: 0

                        Text {
                            Layout.bottomMargin: 8
                            text: win.pageLabel()
                            color: Theme.foreground
                            font.family: Theme.font
                            font.pixelSize: 18
                            font.bold: true
                        }

                        ColumnLayout {
                            id: vozPage
                            Layout.fillWidth: true
                            visible: win.page === "voz"
                            spacing: 6

                                Meter { Layout.fillWidth: true; label: "entrada"; level: win.st.in || 0 }
                                Meter { Layout.fillWidth: true; label: "saída"; level: win.st.out || 0 }

                            History {
                                id: historyTop
                                Layout.fillWidth: true
                                Layout.topMargin: 12
                                Layout.preferredHeight: win.wide ? 220 : 160
                                input: win.st.in || 0
                                output: win.st.out || 0
                            }

                            // Recursos usados durante a call ficam na mesma tela, em cards.
                            GridLayout {
                                Layout.fillWidth: true
                                Layout.topMargin: 16
                                columns: win.wide ? 2 : 1
                                columnSpacing: Theme.gap
                                rowSpacing: Theme.gap

                                Card {
                                    Layout.fillWidth: true
                                    Layout.alignment: Qt.AlignTop
                                    title: "Retorno no fone"
                                    dot: win.mon.active ? Theme.accent : "transparent"

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

                                }
                                Card {
                                    Layout.fillWidth: true
                                    Layout.alignment: Qt.AlignTop
                                    title: "Teste de voz"
                                    dot: win.rec.can_stop ? Theme.urgent : "transparent"

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

                                }
                                Card {
                                    Layout.fillWidth: true
                                    Layout.alignment: Qt.AlignTop
                                    title: "Fonte extra"
                                    dot: (win.st.extra || {}).linked ? Theme.accent : "transparent"

                                    Text {
                                        Layout.fillWidth: true
                                        text: {
                                            const x = win.st.extra || {}
                                            const sel = (x.streams || []).find(s => s.id === x.selected)
                                            if (x.error) return x.error
                                            if (sel && sel.call) return "Cuidado: isso devolve o som da call para ela mesma (eco)"
                                            if (x.linked) return "No microfone virtual  ·  você continua ouvindo no fone"
                                            if (!(x.streams || []).length) return "Nenhum app tocando som agora"
                                            return "Escolha um app para mandar o som dele junto com a voz"
                                        }
                                        color: Theme.muted
                                        font.family: Theme.font
                                        font.pixelSize: 11
                                        wrapMode: Text.Wrap
                                    }
                                    Flow {
                                        Layout.fillWidth: true
                                        spacing: 6
                                        Repeater {
                                            model: (win.st.extra || {}).streams || []
                                            delegate: Chip {
                                                required property var modelData
                                                text: (modelData.call ? "⚠ " : "")
                                                    + (modelData.label.length > 48 ? modelData.label.slice(0, 47) + "…" : modelData.label)
                                                active: modelData.id === (win.st.extra || {}).selected
                                                onClicked: backend.send({ c: "extra_source", id: active ? null : modelData.id })
                                            }
                                        }
                                    }
                                    Slider {
                                        Layout.fillWidth: true
                                        title: "Volume da fonte extra"
                                        from: 0; to: 100; step: 1; unit: "%"
                                        value: (win.st.extra || {}).volume || 0
                                        enabled: (win.st.extra || {}).selected != null
                                        onMoved: v => backend.send({ c: "extra_volume", value: v })
                                    }
                                }
                            }
                        }
                        ColumnLayout {
                            Layout.fillWidth: true
                            visible: win.page === "perfil"
                            spacing: 6

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
                        }
                        ColumnLayout {
                            Layout.fillWidth: true
                            visible: win.page === "efeitos"
                            spacing: 8

                            Text {
                                Layout.fillWidth: true
                                text: "Os canais seguem a ordem do sinal, da esquerda para a direita."
                                color: Theme.muted
                                font.family: Theme.font
                                font.pixelSize: 11
                                wrapMode: Text.Wrap
                            }

                            // Mesa: rola na horizontal quando a janela é estreita.
                            Flickable {
                                id: desk
                                Layout.fillWidth: true
                                // A mesa ocupa a altura que sobra no painel: faders mais longos, ajuste mais fino.
                                Layout.preferredHeight: Math.max(300, pane.height - 130 - (overflow ? 14 : 0))
                                contentWidth: deskRow.width
                                contentHeight: height
                                flickableDirection: Flickable.HorizontalFlick
                                boundsBehavior: Flickable.StopAtBounds
                                clip: true
                                readonly property bool overflow: contentWidth > width + 1
                                function scrollBy(dx) {
                                    contentX = Math.max(0, Math.min(contentWidth - width, contentX + dx))
                                }

                                // Roda do mouse e touchpad rolam a mesa na horizontal quando ela não cabe.
                                // Vale a roda vertical comum, o Shift+roda e o deslize lateral do touchpad.
                                WheelHandler {
                                    enabled: desk.overflow
                                    acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
                                    onWheel: event => {
                                        var d = event.pixelDelta.x !== 0 || event.pixelDelta.y !== 0
                                            ? (Math.abs(event.pixelDelta.x) > Math.abs(event.pixelDelta.y) ? event.pixelDelta.x : event.pixelDelta.y)
                                            : (Math.abs(event.angleDelta.x) > Math.abs(event.angleDelta.y) ? event.angleDelta.x : event.angleDelta.y) / 2
                                        desk.scrollBy(-d)
                                    }
                                }

                                RowLayout {
                                    id: deskRow
                                    spacing: 6
                                    // Na janela larga os canais esticam para ocupar a mesa toda.
                                    width: Math.max(implicitWidth, desk.width)
                                    height: desk.height
                                    readonly property var fx: win.st.fx || ({})

                                    Strip {
                                        Layout.fillWidth: true
                                        Layout.fillHeight: true
                                        title: "Graves"
                                        hint: "Corta rumble abaixo de ~80 Hz"
                                        on: win.effect("highpass")
                                        onToggled: on => win.setEffect("highpass", on)
                                    }
                                    Strip {
                                        Layout.fillWidth: true
                                        Layout.fillHeight: true
                                        title: "Ruído"
                                        hint: "RNNoise\nremove o ruído de fundo"
                                        on: win.effect("denoise")
                                        onToggled: on => win.setEffect("denoise", on)
                                    }
                                    Strip {
                                        Layout.fillWidth: true
                                        Layout.fillHeight: true
                                        title: "Gate"
                                        on: win.effect("gate")
                                        led: !on ? "transparent"
                                            : deskRow.fx.gate === "open" ? Theme.ok
                                            : deskRow.fx.gate === "hold" ? Theme.warn
                                            : deskRow.fx.gate === "closed" ? Theme.urgent : Theme.muted
                                        status: !on ? "" : ({ open: "aberto", hold: "espera", closed: "fechado" })[deskRow.fx.gate] || "sem áudio"
                                        onToggled: on => win.setEffect("gate", on)
                                        Fader {
                                            Layout.fillHeight: true
                                            label: "limiar"
                                            from: -80; to: -20; step: 1; unit: " dB"
                                            value: win.tuning("gate_db")
                                            enabled: win.effect("gate")
                                            onMoved: v => backend.send({ c: "tune", name: "gate_db", value: v })
                                        }
                                    }
                                    Strip {
                                        Layout.fillWidth: true
                                        Layout.fillHeight: true
                                        title: "De-esser"
                                        hint: "Suaviza chiados de \"s\""
                                        on: win.effect("deesser")
                                        onToggled: on => win.setEffect("deesser", on)
                                    }
                                    Strip {
                                        Layout.fillWidth: true
                                        Layout.fillHeight: true
                                        title: "Presença"
                                        hint: "Realça a clareza da voz (~3 kHz)"
                                        on: win.effect("presence")
                                        onToggled: on => win.setEffect("presence", on)
                                    }
                                    // Equalizador de 3 bandas em dB; duplo clique zera a banda.
                                    Strip {
                                        Layout.fillWidth: true
                                        Layout.fillHeight: true
                                        title: "EQ"
                                        on: win.effect("eq")
                                        onToggled: on => win.setEffect("eq", on)
                                        Fader {
                                            Layout.fillHeight: true
                                            label: "grave"
                                            from: -12; to: 12; step: 0.5; unit: ""; signed: true; home: 0
                                            value: win.tuning("eq_low")
                                            enabled: win.effect("eq")
                                            onMoved: v => backend.send({ c: "tune", name: "eq_low", value: v })
                                        }
                                        Fader {
                                            Layout.fillHeight: true
                                            label: "médio"
                                            from: -12; to: 12; step: 0.5; unit: ""; signed: true; home: 0
                                            value: win.tuning("eq_mid")
                                            enabled: win.effect("eq")
                                            onMoved: v => backend.send({ c: "tune", name: "eq_mid", value: v })
                                        }
                                        Fader {
                                            Layout.fillHeight: true
                                            label: "agudo"
                                            from: -12; to: 12; step: 0.5; unit: ""; signed: true; home: 0
                                            value: win.tuning("eq_high")
                                            enabled: win.effect("eq")
                                            onMoved: v => backend.send({ c: "tune", name: "eq_high", value: v })
                                        }
                                    }
                                    Strip {
                                        Layout.fillWidth: true
                                        Layout.fillHeight: true
                                        title: "Ganho"
                                        on: win.effect("auto_gain")
                                        onText: "AUTO"
                                        offText: "MANUAL"
                                        // Em manual, o canal segue ativo: o LED fica aceso mesmo assim.
                                        led: Theme.ok
                                        onToggled: on => win.setEffect("auto_gain", on)
                                        // O ganho manual só vale com o volume automático desligado.
                                        Fader {
                                            Layout.fillHeight: true
                                            label: "manual"
                                            from: 0; to: 24; step: 1; unit: " dB"
                                            value: win.tuning("makeup_db")
                                            enabled: !win.effect("auto_gain")
                                            onMoved: v => backend.send({ c: "tune", name: "makeup_db", value: v })
                                        }
                                        VMeter {
                                            Layout.fillHeight: true
                                            label: "auto"
                                            center: true
                                            live: deskRow.fx.auto_gain_db != null
                                            fraction: (deskRow.fx.auto_gain_db || 0) / 12
                                            readout: {
                                                var v = deskRow.fx.auto_gain_db || 0
                                                return (v >= 0 ? "+" : "") + v.toFixed(1).replace(".", ",")
                                            }
                                        }
                                    }
                                    Strip {
                                        Layout.fillWidth: true
                                        Layout.fillHeight: true
                                        title: "Compressor"
                                        on: win.effect("compressor")
                                        led: on && deskRow.fx.reduction_db > 0.5 ? Theme.warn : (on ? Theme.ok : "transparent")
                                        onToggled: on => win.setEffect("compressor", on)
                                        Fader {
                                            Layout.fillHeight: true
                                            label: "limiar"
                                            from: -50; to: 0; step: 1; unit: " dB"
                                            value: win.tuning("threshold_db")
                                            enabled: win.effect("compressor")
                                            onMoved: v => backend.send({ c: "tune", name: "threshold_db", value: v })
                                        }
                                        Fader {
                                            Layout.fillHeight: true
                                            label: "razão"
                                            from: 1; to: 10; step: 0.5; unit: ":1"
                                            value: win.tuning("ratio")
                                            enabled: win.effect("compressor")
                                            onMoved: v => backend.send({ c: "tune", name: "ratio", value: v })
                                        }
                                        // Redução de ganho (GR): desce do topo, como nas mesas.
                                        VMeter {
                                            Layout.fillHeight: true
                                            label: "GR"
                                            fromTop: true
                                            color: Theme.warn
                                            live: deskRow.fx.reduction_db != null
                                            fraction: (deskRow.fx.reduction_db || 0) / 20
                                            readout: "-" + (deskRow.fx.reduction_db || 0).toFixed(1).replace(".", ",")
                                        }
                                    }

                                    // Master: níveis de entrada e saída e o silenciar.
                                    Rectangle {
                                        Layout.fillHeight: true
                                        width: 1
                                        color: Theme.surface
                                    }
                                    Strip {
                                        Layout.fillHeight: true
                                        title: "Saída"
                                        on: !(win.st.muted || false)
                                        onText: "AO VIVO"
                                        offText: "MUDO"
                                        led: on ? ((win.st.out || 0) > 0.001 ? Theme.ok : "transparent") : Theme.urgent
                                        onToggled: on => backend.send({ c: "mute", on: !on })
                                        VMeter {
                                            Layout.fillHeight: true
                                            label: "in"
                                            readonly property real db: (win.st.in || 0) > 0 ? 20 * Math.log(win.st.in) / Math.LN10 : -99
                                            fraction: (db + 60) / 60
                                            color: fraction > 0.9 ? Theme.urgent : Theme.accent
                                            readout: db > -60 ? db.toFixed(0) : "-∞"
                                        }
                                        VMeter {
                                            Layout.fillHeight: true
                                            label: "out"
                                            readonly property real db: (win.st.out || 0) > 0 ? 20 * Math.log(win.st.out) / Math.LN10 : -99
                                            fraction: (db + 60) / 60
                                            color: fraction > 0.9 ? Theme.urgent : Theme.accent
                                            readout: db > -60 ? db.toFixed(0) : "-∞"
                                        }
                                    }
                                }
                            }

                            // Barra de rolagem da mesa: arrastável, e um clique no trilho pula até ali.
                            Rectangle {
                                id: deskBar
                                Layout.fillWidth: true
                                implicitHeight: 8
                                radius: 4
                                visible: desk.overflow
                                color: Qt.rgba(Theme.surface.r, Theme.surface.g, Theme.surface.b, 0.5)
                                readonly property real ratio: desk.width / Math.max(1, desk.contentWidth)
                                Rectangle {
                                    id: deskThumb
                                    height: parent.height
                                    radius: 4
                                    width: Math.max(40, parent.width * deskBar.ratio)
                                    x: (parent.width - width) * (desk.contentX / Math.max(1, desk.contentWidth - desk.width))
                                    color: thumbArea.pressed || thumbArea.containsMouse ? Theme.accent : Theme.muted
                                }
                                MouseArea {
                                    id: thumbArea
                                    anchors.fill: parent
                                    anchors.margins: -6
                                    hoverEnabled: true
                                    cursorShape: Qt.PointingHandCursor
                                    preventStealing: true
                                    property real grab: 0
                                    function seek(mx) {
                                        var f = (mx - 6 - grab) / Math.max(1, deskBar.width - deskThumb.width)
                                        desk.contentX = Math.max(0, Math.min(1, f)) * (desk.contentWidth - desk.width)
                                    }
                                    onPressed: mouse => {
                                        var tx = mouse.x - 6
                                        // No polegar, segura pelo ponto clicado; fora dele, centraliza.
                                        grab = tx >= deskThumb.x && tx <= deskThumb.x + deskThumb.width ? tx - deskThumb.x : deskThumb.width / 2
                                        seek(mouse.x)
                                    }
                                    onPositionChanged: mouse => { if (pressed) seek(mouse.x) }
                                }
                            }
                        }
                        ColumnLayout {
                            Layout.fillWidth: true
                            visible: win.page === "conexao"
                            spacing: 6

                            GridLayout {
                                Layout.fillWidth: true
                                columns: win.wide ? 2 : 1
                                columnSpacing: 48
                                rowSpacing: 6
                                uniformCellWidths: true
                                ColumnLayout {
                                    Layout.fillWidth: true
                                    Layout.alignment: Qt.AlignTop
                                    spacing: 6
                        QrCode {
                            Layout.alignment: Qt.AlignHCenter
                            Layout.preferredWidth: win.wide ? 300 : 200
                            Layout.preferredHeight: win.wide ? 300 : 200
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
                                }
                                ColumnLayout {
                                    Layout.fillWidth: true
                                    Layout.alignment: Qt.AlignTop
                                    spacing: 6
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
                        }
                    }
                }
            }
        }
    }
}
