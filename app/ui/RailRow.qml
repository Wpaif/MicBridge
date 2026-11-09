import QtQuick

// Uma linha do trilho lateral, como o trilho de configurações do flea: ícone e nome,
// e na escolhida o fundo em destaque com a barra fina na borda esquerda.
Item {
    id: root
    property string glyph: ""
    property string label: ""
    property bool current: false
    // Cor da bolinha à direita (algo ligado nessa seção); "transparent" esconde.
    property color badge: "transparent"
    // Destaque em vermelho em vez do acento (ex.: microfone silenciado).
    property bool alert: false
    signal clicked()

    readonly property color tint: alert ? Theme.urgent : Theme.accent
    implicitHeight: 36

    Rectangle {
        anchors.fill: parent
        color: root.current ? Qt.rgba(root.tint.r, root.tint.g, root.tint.b, 0.16)
            : area.containsMouse ? Theme.surface : "transparent"
    }
    Rectangle {
        anchors { left: parent.left; top: parent.top; bottom: parent.bottom }
        width: 2
        visible: root.current
        color: root.tint
    }
    Text {
        id: mark
        anchors { left: parent.left; leftMargin: 18; verticalCenter: parent.verticalCenter }
        width: 18
        text: root.glyph
        color: root.current ? root.tint : Theme.muted
        font.family: Theme.font
        font.pixelSize: 15
    }
    Text {
        anchors { left: mark.right; leftMargin: 10; right: dot.left; rightMargin: 8; verticalCenter: parent.verticalCenter }
        text: root.label
        color: Theme.foreground
        font.family: Theme.font
        font.pixelSize: 13
        elide: Text.ElideRight
    }
    Rectangle {
        id: dot
        anchors { right: parent.right; rightMargin: 14; verticalCenter: parent.verticalCenter }
        width: 7; height: 7; radius: 4
        color: root.badge
    }
    MouseArea {
        id: area
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: root.clicked()
    }
}
