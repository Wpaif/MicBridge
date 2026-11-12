import QtQuick
import QtQuick.Layouts

// Confirmação dentro da janela, no lugar dos diálogos do libadwaita.
Rectangle {
    id: root
    property string title: ""
    property string body: ""
    property string action: I18n.t("common.confirm")
    signal accepted()
    signal rejected()

    implicitHeight: box.implicitHeight + 20
    // Aberta pelo teclado, já recebe o foco no Cancelar (o lado seguro).
    onVisibleChanged: if (visible) cancel.forceActiveFocus()
    radius: Theme.radius
    color: "transparent"
    border.color: Theme.urgent

    ColumnLayout {
        id: box
        anchors { fill: parent; margins: 10 }
        spacing: 6
        Text {
            text: root.title
            color: Theme.foreground
            font.family: Theme.font
            font.pixelSize: 13
            font.bold: true
        }
        Text {
            Layout.fillWidth: true
            visible: root.body !== ""
            text: root.body
            color: Theme.muted
            font.family: Theme.font
            font.pixelSize: 11
            wrapMode: Text.Wrap
        }
        Row {
            spacing: 6
            Button { id: cancel; text: I18n.t("common.cancel"); onClicked: root.rejected() }
            Button { text: root.action; accent: true; onClicked: root.accepted() }
        }
    }
}
