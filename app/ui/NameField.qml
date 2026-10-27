import QtQuick
import QtQuick.Layouts

// Campo de nome com Salvar/Cancelar; Enter salva e Esc cancela.
RowLayout {
    id: root
    property alias text: input.text
    property string action: "Salvar"
    signal accepted(string name)
    signal rejected()
    spacing: 6

    function focusInput() { input.forceActiveFocus(); input.selectAll() }

    Rectangle {
        Layout.fillWidth: true
        implicitHeight: 30
        radius: Theme.radius
        color: Theme.surface
        border.color: input.activeFocus ? Theme.accent : Theme.surface
        TextInput {
            id: input
            anchors { fill: parent; leftMargin: 10; rightMargin: 10 }
            verticalAlignment: TextInput.AlignVCenter
            color: Theme.foreground
            selectionColor: Theme.accent
            selectedTextColor: Theme.background
            font.family: Theme.font
            font.pixelSize: 12
            maximumLength: 60
            clip: true
            Keys.onReturnPressed: root.accepted(text)
            Keys.onEnterPressed: root.accepted(text)
            Keys.onEscapePressed: root.rejected()
        }
    }
    Button { text: "Cancelar"; onClicked: root.rejected() }
    Button { text: root.action; accent: true; enabled: input.text.trim() !== ""; onClicked: root.accepted(input.text) }
}
