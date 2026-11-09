// Teclado nos controles: rode com
//   QT_QPA_PLATFORM=offscreen /usr/lib/qt6/bin/qmltestrunner -input app/ui/tests
import QtQuick
import QtTest
Item {
    width: 600; height: 300
    property var sent: []
    Column {
        Toggle { id: tog; width: 500; title: "t"; onToggled: on => checked = on }
        Slider { id: sl; width: 500; from: 0; to: 100; step: 1; value: 25; onMoved: v => sent.push(v) }
    }
    TestCase {
        name: "keys"; when: windowShown
        function test_slider() {
            sl.forceActiveFocus()
            keyClick(Qt.Key_Left); keyClick(Qt.Key_Left)
            compare(sl.shown, 23)
            keyClick(Qt.Key_Right)
            compare(sl.shown, 24)
            keyClick(Qt.Key_End); compare(sl.shown, 100)
            keyClick(Qt.Key_PageDown); compare(sl.shown, 90)
            keyClick(Qt.Key_Home); compare(sl.shown, 0)
            compare(sent, [24, 23, 24, 100, 90, 0])
        }
        function test_toggle_space() {
            tog.forceActiveFocus(); keyClick(Qt.Key_Space); verify(tog.checked)
            keyClick(Qt.Key_Tab); verify(sl.activeFocus)
        }
    }
}
