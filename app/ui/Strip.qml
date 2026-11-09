import QtQuick
import QtQuick.Layouts

// Canal da mesa de efeitos: nome e LED no topo, controles no meio e o botão ON
// embaixo, como numa mesa de som. O LED acende com o efeito ligado e muda de cor
// conforme a atuação (`led`); `status` é a leitura curta ao lado dele.
Rectangle {
    id: root
    property string title: ""
    property string hint: ""
    property bool on: false
    property color led: on ? Theme.ok : "transparent"
    property string status: ""
    // Texto do botão nos dois estados (o canal de ganho usa AUTO/MANUAL).
    property string onText: "ON"
    property string offText: "OFF"
    default property alias content: middle.data
    signal toggled(bool on)

    // Canais só com liga/desliga são estreitos; os com faders crescem com eles.
    implicitWidth: Math.max(70, middle.implicitWidth + 16)
    implicitHeight: 300
    radius: Theme.radius
    color: Qt.rgba(Theme.surface.r, Theme.surface.g, Theme.surface.b, root.on ? 0.55 : 0.25)
    border.width: 1
    border.color: Theme.surface

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 8
        spacing: 6

        Text {
            Layout.fillWidth: true
            horizontalAlignment: Text.AlignHCenter
            text: root.title
            color: root.on ? Theme.foreground : Theme.muted
            font.family: Theme.font
            font.pixelSize: 10
            font.bold: true
            font.letterSpacing: 0
            font.capitalization: Font.AllUppercase
            elide: Text.ElideRight
        }
        RowLayout {
            Layout.alignment: Qt.AlignHCenter
            spacing: 6
            Rectangle {
                width: 8; height: 8; radius: 4
                color: root.led.a > 0 ? root.led : "transparent"
                border.color: root.led.a > 0 ? root.led : Theme.muted
                // Halo discreto quando aceso.
                Rectangle {
                    anchors.centerIn: parent
                    width: 16; height: 16; radius: 8
                    color: root.led
                    opacity: root.led.a > 0 ? 0.25 : 0
                    z: -1
                }
            }
            Text {
                text: root.status
                color: Theme.muted
                font.family: Theme.font
                font.pixelSize: 10
                visible: text !== ""
            }
        }

        Item {
            Layout.fillWidth: true
            Layout.fillHeight: true
            RowLayout {
                id: middle
                anchors { top: parent.top; bottom: parent.bottom; horizontalCenter: parent.horizontalCenter }
                spacing: 6
            }
            // Canais sem ajuste mostram só a descrição no meio.
            Text {
                anchors.centerIn: parent
                width: parent.width
                visible: middle.children.length === 0
                horizontalAlignment: Text.AlignHCenter
                text: root.hint
                color: Theme.muted
                font.family: Theme.font
                font.pixelSize: 10
                wrapMode: Text.Wrap
            }
        }

        // Botão ON, iluminado quando ligado.
        Rectangle {
            id: button
            Layout.fillWidth: true
            implicitHeight: 28
            radius: Theme.radius
            color: root.on ? Theme.accent : (area.containsMouse ? Theme.surface : "transparent")
            border.color: root.activeFocus ? Theme.foreground : (root.on ? Theme.accent : Theme.muted)
            border.width: root.activeFocus ? 2 : 1
            Text {
                anchors.centerIn: parent
                text: root.on ? root.onText : root.offText
                color: root.on ? Theme.background : Theme.muted
                font.family: Theme.font
                font.pixelSize: 11
                font.bold: true
            }
            MouseArea {
                id: area
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: root.toggled(!root.on)
            }
        }
    }

    activeFocusOnTab: visible
    Keys.onSpacePressed: root.toggled(!root.on)
    Keys.onReturnPressed: root.toggled(!root.on)
    Keys.onEnterPressed: root.toggled(!root.on)
}
