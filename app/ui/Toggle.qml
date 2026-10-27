import QtQuick

// Linha clicável com interruptor, no estilo das linhas de configuração do flea.
Rectangle {
    id: root
    property string title: ""
    property string hint: ""
    property bool checked: false
    property bool enabled: true
    signal toggled(bool on)

    implicitHeight: 40
    radius: Theme.radius
    color: area.containsMouse ? Theme.surface : "transparent"
    opacity: enabled ? 1 : 0.5

    Column {
        anchors { left: parent.left; leftMargin: 10; right: knob.left; verticalCenter: parent.verticalCenter }
        Text { text: root.title; color: Theme.foreground; font.family: Theme.font; font.pixelSize: 13 }
        Text { text: root.hint; color: Theme.muted; font.family: Theme.font; font.pixelSize: 11; visible: root.hint !== ""; elide: Text.ElideRight; width: parent.width }
    }
    Rectangle {
        id: knob
        anchors { right: parent.right; rightMargin: 10; verticalCenter: parent.verticalCenter }
        width: 34; height: 18; radius: 9
        color: root.checked ? Theme.accent : Theme.surface
        border.color: root.checked ? Theme.accent : Theme.muted
        Rectangle {
            width: 12; height: 12; radius: 6
            y: 3
            x: root.checked ? parent.width - width - 3 : 3
            color: root.checked ? Theme.background : Theme.muted
            Behavior on x { NumberAnimation { duration: 120 } }
        }
    }
    MouseArea {
        id: area
        anchors.fill: parent
        hoverEnabled: true
        enabled: root.enabled
        cursorShape: Qt.PointingHandCursor
        onClicked: root.toggled(!root.checked)
    }
}
