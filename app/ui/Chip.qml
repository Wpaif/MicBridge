import QtQuick

// Opção selecionável de uma lista curta (perfis, saídas de áudio).
Rectangle {
    id: root
    property string text: ""
    property bool active: false
    property bool enabled: true
    signal clicked()

    width: label.implicitWidth + 24
    height: 30
    radius: Theme.radius
    opacity: enabled || active ? 1 : 0.4
    color: active ? Theme.accent : (area.containsMouse && enabled ? Theme.surface : "transparent")
    border.color: active ? Theme.accent : Theme.surface

    Text {
        id: label
        anchors.centerIn: parent
        text: root.text
        color: root.active ? Theme.background : Theme.foreground
        font.family: Theme.font
        font.pixelSize: 12
    }
    MouseArea {
        id: area
        anchors.fill: parent
        hoverEnabled: true
        enabled: root.enabled
        cursorShape: Qt.PointingHandCursor
        onClicked: root.clicked()
    }
}
