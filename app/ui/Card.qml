import QtQuick
import QtQuick.Layouts

// Bloco com título e borda discreta para agrupar controles de um mesmo recurso.
// O conteúdo vai para uma coluna interna; `dot` acende um ponto ao lado do título
// quando o recurso está ativo (ex.: retorno ligado, gravação em andamento).
Rectangle {
    id: card
    property string title: ""
    property color dot: "transparent"
    default property alias content: body.data

    implicitHeight: body.implicitHeight + 28
    radius: Theme.radius
    color: Qt.rgba(Theme.surface.r, Theme.surface.g, Theme.surface.b, 0.45)
    border.width: 1
    border.color: Theme.surface

    ColumnLayout {
        id: body
        anchors { left: parent.left; right: parent.right; top: parent.top; margins: 14 }
        spacing: 6

        RowLayout {
            Layout.fillWidth: true
            spacing: 8
            Section { text: card.title; topPadding: 0 }
            Rectangle {
                width: 7; height: 7; radius: 3.5
                color: card.dot
                visible: card.dot.a > 0
            }
            Item { Layout.fillWidth: true }
        }
    }
}
