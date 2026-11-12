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
        title: I18n.t("export.title")
        fileMode: FileDialog.SaveFile
        acceptLabel: I18n.t("export.accept")
        nameFilters: [I18n.t("export.filter")]
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
            { id: "voz", label: I18n.t("page.voz"), glyph: "\u{f036c}" },
            { id: "perfil", label: I18n.t("page.perfil"), glyph: "\u{f0004}" },
            { id: "efeitos", label: I18n.t("page.efeitos"), glyph: "\u{f062e}" },
            { id: "conexao", label: I18n.t("page.conexao"), glyph: "\u{f011c}" }
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
                        Text { text: I18n.t("help.title"); color: Theme.foreground; font.family: Theme.font; font.pixelSize: 15; font.bold: true; bottomPadding: 6 }
                        Repeater {
                            model: [
                                ["Tab  ·  ↑ ↓", I18n.t("help.next")],
                                ["← →", I18n.t("help.adjust")],
                                ["PgUp PgDn  ·  Home End", I18n.t("help.slider")],
                                [I18n.t("help.key.space_enter"), I18n.t("help.toggle")],
                                ["Ctrl+Tab  ·  Alt+↑ ↓", I18n.t("help.section")],
                                ["Alt+1 … 4", I18n.t("help.goto")],
                                ["M", I18n.t("help.mute")],
                                ["1 … 9", I18n.t("help.profile")],
                                ["N  ·  S  ·  R  ·  Del", I18n.t("help.profile_edit")],
                                ["P", I18n.t("help.pairing")],
                                ["Esc", I18n.t("help.esc")],
                                ["?  ·  F1", I18n.t("help.sheet")]
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
                text: I18n.t("waiting")
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
                            text: I18n.t("rail.shortcuts")
                            color: Theme.muted
                            font.family: Theme.font
                            font.pixelSize: 11
                            MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: win.helpOpen = true }
                        }

                        // Silenciar fica sempre à mão, em qualquer seção.
                        RailRow {
                            Layout.fillWidth: true
                            glyph: win.st.muted ? "\u{f036d}" : "\u{f036c}"
                            label: win.st.muted ? I18n.t("rail.muted") : I18n.t("rail.mute")
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

                                Meter { Layout.fillWidth: true; label: I18n.t("meter.in"); level: win.st.in || 0; clip: !!(win.st.clip && win.st.clip.in) }
                                Meter { Layout.fillWidth: true; label: I18n.t("meter.out"); level: win.st.out || 0; clip: !!(win.st.clip && win.st.clip.out) }
                                Text {
                                    Layout.fillWidth: true
                                    readonly property bool hotIn: !!(win.st.clip && win.st.clip.in)
                                    readonly property bool hotOut: !!(win.st.clip && win.st.clip.out)
                                    visible: hotIn || hotOut
                                    wrapMode: Text.WordWrap
                                    color: Theme.urgent
                                    font.family: Theme.font
                                    font.pixelSize: 12
                                    text: hotIn ? I18n.t("clip.in") : I18n.t("clip.out")
                                }

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
                                    title: I18n.t("monitor.title")
                                    dot: win.mon.active ? Theme.accent : "transparent"

                                        Toggle {
                                            Layout.fillWidth: true
                                            title: I18n.t("monitor.listen")
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
                                                text: I18n.t("monitor.refresh")
                                                enabled: win.mon.can_refresh || false
                                                onClicked: backend.send({ c: "monitor_refresh" })
                                            }
                                            Button {
                                                text: I18n.t("monitor.retry")
                                                accent: true
                                                visible: win.mon.can_retry || false
                                                onClicked: backend.send({ c: "monitor_retry" })
                                            }
                                        }
                                        Slider {
                                            Layout.fillWidth: true
                                            title: I18n.t("monitor.volume")
                                            from: 0; to: 100; step: 1; unit: "%"
                                            value: win.mon.volume || 0
                                            onMoved: v => backend.send({ c: "monitor_volume", value: v })
                                        }
                                        Text {
                                            Layout.fillWidth: true
                                            Layout.leftMargin: 10
                                            text: I18n.t("monitor.exclusive")
                                            color: Theme.muted
                                            font.family: Theme.font
                                            font.pixelSize: 11
                                            wrapMode: Text.Wrap
                                        }

                                }
                                Card {
                                    Layout.fillWidth: true
                                    Layout.alignment: Qt.AlignTop
                                    title: I18n.t("rec.title")
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
                                                text: win.rec.can_stop ? I18n.t("rec.stop") : I18n.t("rec.start")
                                                accent: win.rec.can_stop || false
                                                enabled: win.rec.can_start || win.rec.can_stop || false
                                                onClicked: backend.send({ c: "record", on: !win.rec.can_stop })
                                            }
                                            Chip {
                                                text: I18n.t("rec.play_original")
                                                active: win.rec.playing === "original"
                                                enabled: win.rec.can_listen || false
                                                onClicked: backend.send(active ? { c: "stop_play" } : { c: "play", version: "original" })
                                            }
                                            Chip {
                                                text: I18n.t("rec.play_treated")
                                                active: win.rec.playing === "treated"
                                                enabled: win.rec.can_listen || false
                                                onClicked: backend.send(active ? { c: "stop_play" } : { c: "play", version: "treated" })
                                            }
                                            Button {
                                                text: I18n.t("rec.export")
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
                                            title: I18n.t("rec.replace_title")
                                            body: win.rec.confirm ? I18n.t("rec.replace_body", { original: win.rec.confirm.original, treated: win.rec.confirm.treated }) : ""
                                            action: I18n.t("rec.replace")
                                            onAccepted: backend.send({ c: "export_confirm", on: true })
                                            onRejected: backend.send({ c: "export_confirm", on: false })
                                        }

                                }
                                Card {
                                    Layout.fillWidth: true
                                    Layout.alignment: Qt.AlignTop
                                    title: I18n.t("extra.title")
                                    dot: (win.st.extra || {}).linked ? Theme.accent : "transparent"

                                    Text {
                                        Layout.fillWidth: true
                                        text: {
                                            const x = win.st.extra || {}
                                            const sel = (x.streams || []).find(s => s.id === x.selected)
                                            if (x.error) return x.error
                                            if (sel && sel.call) return I18n.t("extra.echo")
                                            if (x.linked) return I18n.t("extra.linked")
                                            if (!(x.streams || []).length) return I18n.t("extra.none")
                                            return I18n.t("extra.choose")
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
                                        title: I18n.t("extra.volume")
                                        from: 0; to: 100; step: 1; unit: "%"
                                        value: (win.st.extra || {}).volume || 0
                                        enabled: (win.st.extra || {}).selected != null
                                        onMoved: v => backend.send({ c: "extra_volume", value: v })
                                    }
                                }
                                // Calibração: mede ruído e voz e sugere gate, redução de ruído e ganho.
                                Card {
                                    id: calCard
                                    Layout.fillWidth: true
                                    Layout.alignment: Qt.AlignTop
                                    title: I18n.t("cal.title")
                                    readonly property var cal: win.st.calibration || ({ phase: "idle" })
                                    readonly property var d: cal.detail || ({})
                                    readonly property bool running: cal.phase === "silence" || cal.phase === "speech"
                                    dot: running ? Theme.warn : "transparent"

                                    Text {
                                        Layout.fillWidth: true
                                        visible: calCard.cal.phase === "idle"
                                        text: win.link === "connected"
                                            ? I18n.t("cal.intro")
                                            : I18n.t("cal.connect")
                                        color: Theme.muted
                                        font.family: Theme.font
                                        font.pixelSize: 11
                                        wrapMode: Text.Wrap
                                    }

                                    // Em andamento: instrução da fase e progresso.
                                    Text {
                                        Layout.fillWidth: true
                                        visible: calCard.running
                                        text: calCard.cal.phase === "silence"
                                            ? I18n.t("cal.silence")
                                            : I18n.t("cal.speech")
                                        color: Theme.foreground
                                        font.family: Theme.font
                                        font.pixelSize: 12
                                        wrapMode: Text.Wrap
                                    }
                                    Text {
                                        Layout.fillWidth: true
                                        visible: calCard.cal.phase === "speech"
                                        text: I18n.t("cal.script")
                                        color: Theme.accent
                                        font.family: Theme.font
                                        font.pixelSize: 12
                                        wrapMode: Text.Wrap
                                    }
                                    Rectangle {
                                        Layout.fillWidth: true
                                        visible: calCard.running
                                        height: 4
                                        radius: 2
                                        color: Theme.surface
                                        Rectangle {
                                            width: parent.width * (calCard.cal.progress || 0)
                                            height: parent.height
                                            radius: 2
                                            color: calCard.cal.phase === "silence" ? Theme.muted : Theme.accent
                                        }
                                    }

                                    // Resultado: o que mudaria, de -> para.
                                    Column {
                                        Layout.fillWidth: true
                                        visible: calCard.cal.phase === "done"
                                        spacing: 4
                                        Repeater {
                                            model: {
                                                const d = calCard.d
                                                if (calCard.cal.phase !== "done") return []
                                                const rows = [[I18n.t("cal.measured"), I18n.t("cal.measured_value", { noise: d.noise_db, voice: d.voice_db })]]
                                                rows.push([I18n.t("cal.gate"), d.gate_was_on
                                                    ? (d.gate_from === d.gate_db ? I18n.t("cal.same_db", { db: d.gate_db }) : I18n.t("cal.change_db", { from: d.gate_from, to: d.gate_db }))
                                                    : I18n.t("cal.turn_on_db", { db: d.gate_db })])
                                                if (d.denoise !== d.denoise_from) rows.push([I18n.t("cal.denoise"), I18n.t("cal.turn_on")])
                                                if (d.makeup_db !== null && d.makeup_db !== undefined)
                                                    rows.push([I18n.t("cal.makeup"), I18n.t("cal.change_db", { from: d.makeup_from, to: d.makeup_db })])
                                                return rows
                                            }
                                            delegate: Row {
                                                required property var modelData
                                                spacing: 8
                                                Text { width: 130; text: modelData[0]; color: Theme.muted; font.family: Theme.font; font.pixelSize: 11 }
                                                Text { text: modelData[1]; color: Theme.foreground; font.family: Theme.font; font.pixelSize: 12 }
                                            }
                                        }
                                    }
                                    Text {
                                        Layout.fillWidth: true
                                        visible: (calCard.cal.phase === "done" && !!calCard.d.warning) || calCard.cal.phase === "failed"
                                        text: calCard.cal.phase === "failed" ? (calCard.cal.detail || "") : (calCard.d.warning || "")
                                        color: calCard.cal.phase === "failed" ? Theme.urgent : Theme.warn
                                        font.family: Theme.font
                                        font.pixelSize: 11
                                        wrapMode: Text.Wrap
                                    }

                                    Flow {
                                        Layout.fillWidth: true
                                        spacing: 6
                                        Button {
                                            visible: calCard.cal.phase === "idle" || calCard.cal.phase === "failed"
                                            text: calCard.cal.phase === "failed" ? I18n.t("cal.retry") : I18n.t("cal.start")
                                            accent: true
                                            enabled: win.link === "connected"
                                            onClicked: backend.send({ c: "calibrate", on: true })
                                        }
                                        Button {
                                            visible: calCard.running
                                            text: I18n.t("common.cancel")
                                            onClicked: backend.send({ c: "calibrate", on: false })
                                        }
                                        Button {
                                            visible: calCard.cal.phase === "done"
                                            text: I18n.t("cal.apply")
                                            accent: true
                                            onClicked: backend.send({ c: "calibrate_apply" })
                                        }
                                        Button {
                                            visible: calCard.cal.phase === "done" || calCard.cal.phase === "failed"
                                            text: calCard.cal.phase === "done" ? I18n.t("cal.discard") : I18n.t("cal.close")
                                            onClicked: backend.send({ c: "calibrate_discard" })
                                        }
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
                                        text: I18n.t("profile.save")
                                        accent: true
                                        enabled: win.editor.can_save || false
                                        onClicked: backend.send({ c: "profile_save" })
                                    }
                                    Button {
                                        text: I18n.t("profile.new")
                                        enabled: win.editor.can_new || false
                                        onClicked: win.editProfile("new")
                                    }
                                    Button {
                                        text: I18n.t("profile.rename_menu")
                                        enabled: win.editor.can_rename || false
                                        onClicked: win.editProfile("rename")
                                    }
                                    Button {
                                        text: I18n.t("common.delete")
                                        enabled: win.editor.can_delete || false
                                        onClicked: win.profileEdit = "delete"
                                    }
                                }
                                NameField {
                                    id: nameField
                                    Layout.fillWidth: true
                                    visible: win.profileEdit === "new" || win.profileEdit === "rename"
                                    action: win.profileEdit === "rename" ? I18n.t("profile.rename") : I18n.t("profile.create")
                                    onAccepted: name => {
                                        backend.send({ c: win.profileEdit === "rename" ? "profile_rename" : "profile_new", name: name })
                                        win.profileEdit = ""
                                    }
                                    onRejected: win.profileEdit = ""
                                }
                                Confirm {
                                    Layout.fillWidth: true
                                    visible: win.profileEdit === "delete"
                                    title: I18n.t("profile.delete_title")
                                    body: I18n.t("profile.delete_body")
                                    action: I18n.t("common.delete")
                                    onAccepted: { backend.send({ c: "profile_delete" }); win.profileEdit = "" }
                                    onRejected: win.profileEdit = ""
                                }
                                Confirm {
                                    Layout.fillWidth: true
                                    visible: win.pendingProfile !== ""
                                    title: I18n.t("profile.discard_title")
                                    body: I18n.t("profile.discard_body")
                                    action: I18n.t("profile.discard")
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
                                text: I18n.t("fx.order")
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
                                        title: I18n.t("fx.highpass")
                                        hint: I18n.t("fx.highpass_hint")
                                        on: win.effect("highpass")
                                        onToggled: on => win.setEffect("highpass", on)
                                    }
                                    Strip {
                                        Layout.fillWidth: true
                                        Layout.fillHeight: true
                                        title: I18n.t("fx.denoise")
                                        hint: I18n.t("fx.denoise_hint")
                                        on: win.effect("denoise")
                                        onToggled: on => win.setEffect("denoise", on)
                                        // Abaixo de 100 % deixa passar um pouco de ruído em troca de
                                        // preservar sílabas fracas; duplo clique volta a 100 %.
                                        Fader {
                                            Layout.fillHeight: true
                                            label: I18n.t("fx.strength")
                                            from: 0; to: 100; step: 5; unit: "%"; home: 100
                                            value: win.tuning("denoise_strength")
                                            enabled: win.effect("denoise")
                                            onMoved: v => backend.send({ c: "tune", name: "denoise_strength", value: v })
                                        }
                                    }
                                    Strip {
                                        Layout.fillWidth: true
                                        Layout.fillHeight: true
                                        title: I18n.t("fx.gate")
                                        on: win.effect("gate")
                                        led: !on ? "transparent"
                                            : deskRow.fx.gate === "open" ? Theme.ok
                                            : deskRow.fx.gate === "hold" ? Theme.warn
                                            : deskRow.fx.gate === "closed" ? Theme.urgent : Theme.muted
                                        status: !on ? "" : ({ open: I18n.t("fx.gate_open"), hold: I18n.t("fx.gate_hold"), closed: I18n.t("fx.gate_closed") })[deskRow.fx.gate] || I18n.t("fx.gate_silent")
                                        onToggled: on => win.setEffect("gate", on)
                                        Fader {
                                            Layout.fillHeight: true
                                            label: I18n.t("fx.threshold")
                                            from: -80; to: -20; step: 1; unit: " dB"
                                            value: win.tuning("gate_db")
                                            enabled: win.effect("gate")
                                            onMoved: v => backend.send({ c: "tune", name: "gate_db", value: v })
                                        }
                                    }
                                    Strip {
                                        Layout.fillWidth: true
                                        Layout.fillHeight: true
                                        title: I18n.t("fx.deesser")
                                        hint: I18n.t("fx.deesser_hint")
                                        on: win.effect("deesser")
                                        onToggled: on => win.setEffect("deesser", on)
                                    }
                                    Strip {
                                        Layout.fillWidth: true
                                        Layout.fillHeight: true
                                        title: I18n.t("fx.presence")
                                        hint: I18n.t("fx.presence_hint")
                                        on: win.effect("presence")
                                        onToggled: on => win.setEffect("presence", on)
                                    }
                                    // Equalizador de 3 bandas em dB; duplo clique zera a banda.
                                    Strip {
                                        Layout.fillWidth: true
                                        Layout.fillHeight: true
                                        title: I18n.t("fx.eq")
                                        on: win.effect("eq")
                                        onToggled: on => win.setEffect("eq", on)
                                        // Atalhos: um toque liga o EQ e posiciona as três bandas.
                                        ColumnLayout {
                                            Layout.alignment: Qt.AlignVCenter
                                            spacing: 4
                                            Repeater {
                                                model: win.st.eq_presets || []
                                                Chip {
                                                    required property var modelData
                                                    // O Chip só define width/height; o layout precisa do tamanho preferido.
                                                    Layout.preferredWidth: 78
                                                    Layout.preferredHeight: 26
                                                    text: modelData.label
                                                    active: win.st.eq_preset === modelData.id
                                                    onClicked: backend.send({ c: "eq_preset", name: modelData.id })
                                                }
                                            }
                                        }
                                        Fader {
                                            Layout.fillHeight: true
                                            label: I18n.t("fx.eq_low")
                                            from: -12; to: 12; step: 0.5; unit: ""; signed: true; home: 0
                                            value: win.tuning("eq_low")
                                            enabled: win.effect("eq")
                                            onMoved: v => backend.send({ c: "tune", name: "eq_low", value: v })
                                        }
                                        Fader {
                                            Layout.fillHeight: true
                                            label: I18n.t("fx.eq_mid")
                                            from: -12; to: 12; step: 0.5; unit: ""; signed: true; home: 0
                                            value: win.tuning("eq_mid")
                                            enabled: win.effect("eq")
                                            onMoved: v => backend.send({ c: "tune", name: "eq_mid", value: v })
                                        }
                                        Fader {
                                            Layout.fillHeight: true
                                            label: I18n.t("fx.eq_high")
                                            from: -12; to: 12; step: 0.5; unit: ""; signed: true; home: 0
                                            value: win.tuning("eq_high")
                                            enabled: win.effect("eq")
                                            onMoved: v => backend.send({ c: "tune", name: "eq_high", value: v })
                                        }
                                    }
                                    Strip {
                                        Layout.fillWidth: true
                                        Layout.fillHeight: true
                                        title: I18n.t("fx.gain")
                                        on: win.effect("auto_gain")
                                        onText: I18n.t("fx.gain_auto")
                                        offText: I18n.t("fx.gain_manual")
                                        // Em manual, o canal segue ativo: o LED fica aceso mesmo assim.
                                        led: Theme.ok
                                        onToggled: on => win.setEffect("auto_gain", on)
                                        // O ganho manual só vale com o volume automático desligado.
                                        Fader {
                                            Layout.fillHeight: true
                                            label: I18n.t("fx.manual")
                                            from: 0; to: 24; step: 1; unit: " dB"
                                            value: win.tuning("makeup_db")
                                            enabled: !win.effect("auto_gain")
                                            onMoved: v => backend.send({ c: "tune", name: "makeup_db", value: v })
                                        }
                                        VMeter {
                                            Layout.fillHeight: true
                                            label: I18n.t("fx.auto")
                                            center: true
                                            live: deskRow.fx.auto_gain_db != null
                                            fraction: (deskRow.fx.auto_gain_db || 0) / 12
                                            readout: {
                                                var v = deskRow.fx.auto_gain_db || 0
                                                return (v >= 0 ? "+" : "") + I18n.num(v, 1)
                                            }
                                        }
                                    }
                                    Strip {
                                        Layout.fillWidth: true
                                        Layout.fillHeight: true
                                        title: I18n.t("fx.compressor")
                                        on: win.effect("compressor")
                                        led: on && deskRow.fx.reduction_db > 0.5 ? Theme.warn : (on ? Theme.ok : "transparent")
                                        onToggled: on => win.setEffect("compressor", on)
                                        Fader {
                                            Layout.fillHeight: true
                                            label: I18n.t("fx.threshold")
                                            from: -50; to: 0; step: 1; unit: " dB"
                                            value: win.tuning("threshold_db")
                                            enabled: win.effect("compressor")
                                            onMoved: v => backend.send({ c: "tune", name: "threshold_db", value: v })
                                        }
                                        Fader {
                                            Layout.fillHeight: true
                                            label: I18n.t("fx.ratio")
                                            from: 1; to: 10; step: 0.5; unit: ":1"
                                            value: win.tuning("ratio")
                                            enabled: win.effect("compressor")
                                            onMoved: v => backend.send({ c: "tune", name: "ratio", value: v })
                                        }
                                        // Redução de ganho (GR): desce do topo, como nas mesas.
                                        VMeter {
                                            Layout.fillHeight: true
                                            label: I18n.t("fx.gr")
                                            fromTop: true
                                            color: Theme.warn
                                            live: deskRow.fx.reduction_db != null
                                            fraction: (deskRow.fx.reduction_db || 0) / 20
                                            readout: "-" + I18n.num(deskRow.fx.reduction_db || 0, 1)
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
                                        title: I18n.t("fx.output")
                                        on: !(win.st.muted || false)
                                        onText: I18n.t("fx.live")
                                        offText: I18n.t("fx.muted")
                                        led: on ? ((win.st.out || 0) > 0.001 ? Theme.ok : "transparent") : Theme.urgent
                                        onToggled: on => backend.send({ c: "mute", on: !on })
                                        VMeter {
                                            Layout.fillHeight: true
                                            label: "in"
                                            clip: !!(win.st.clip && win.st.clip.in)
                                            readonly property real db: (win.st.in || 0) > 0 ? 20 * Math.log(win.st.in) / Math.LN10 : -99
                                            fraction: (db + 60) / 60
                                            color: fraction > 0.9 ? Theme.urgent : Theme.accent
                                            readout: db > -60 ? db.toFixed(0) : "-∞"
                                        }
                                        VMeter {
                                            Layout.fillHeight: true
                                            label: "out"
                                            clip: !!(win.st.clip && win.st.clip.out)
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
                                ? I18n.t("pair.scan", { time: Math.floor(win.st.pairing / 60) + ":" + String(win.st.pairing % 60).padStart(2, "0") })
                                : I18n.t("pair.paired")
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
                            title: I18n.t("pair.accept")
                            hint: I18n.t("pair.accept_hint")
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
                                text: I18n.plural("pair.devices", win.st.devices)
                                color: Theme.foreground
                                font.family: Theme.font
                                font.pixelSize: 13
                                elide: Text.ElideRight
                            }
                            Button { text: I18n.t("pair.forget"); onClicked: win.forgetting = true }
                        }
                        Confirm {
                            Layout.fillWidth: true
                            visible: win.forgetting
                            title: I18n.t("pair.forget_title")
                            body: I18n.t("pair.forget_body")
                            action: I18n.t("pair.forget")
                            onAccepted: { backend.send({ c: "forget" }); win.forgetting = false }
                            onRejected: win.forgetting = false
                        }
                        Toggle {
                            Layout.fillWidth: true
                            title: I18n.t("autostart")
                            hint: I18n.t("autostart_hint")
                            checked: win.st.autostart || false
                            onToggled: on => backend.send({ c: "autostart", on: on })
                        }
                        // Confira no celular ao aceitar o aviso do certificado.
                        Section { text: I18n.t("cert.title") }
                        RowLayout {
                            Layout.fillWidth: true
                            spacing: 6
                            Text {
                                Layout.fillWidth: true
                                text: win.st.fingerprint || I18n.t("cert.waiting")
                                color: Theme.muted
                                font.family: Theme.font
                                font.pixelSize: 10
                                wrapMode: Text.WrapAnywhere
                            }
                            Button {
                                text: win.copied ? I18n.t("cert.copied") : I18n.t("cert.copy")
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
