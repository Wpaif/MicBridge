import QtQuick

// Botão de texto compacto; `accent` destaca a ação principal.
Rectangle {
    id: root
    property string text: ""
    property bool accent: false
    property bool enabled: true
    signal clicked()

    implicitWidth: label.implicitWidth + 24
    implicitHeight: 30
    radius: Theme.radius
    opacity: enabled ? 1 : 0.4
    color: accent ? Theme.accent : (area.containsMouse && enabled ? Theme.surface : "transparent")
    border.color: accent ? Theme.accent : Theme.surface

    Text {
        id: label
        anchors.centerIn: parent
        text: root.text
        color: root.accent ? Theme.background : Theme.foreground
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
