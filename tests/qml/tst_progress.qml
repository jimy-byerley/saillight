import QtQuick 2.0
import QtTest 1.0
import "../../qml/components"

// the progress row takes room only for work that lasts
TestCase {
    name: "progress"

    QtObject {
        id: fake
        property bool scanning: false
        property bool busy: false
        property string busyText: ""
        property real busyProgress: -1
        property bool busyCancellable: false
        property real scanItems: 0
        property real scanBytes: 0
        function cancelBusy() { }
    }

    Component {
        id: component
        ProgressRow { width: 600 }
    }

    property var row: null

    function init() {
        fake.scanning = false
        fake.busy = false
        row = component.createObject(null, { model: fake })
    }

    function cleanup() {
        row.destroy()
    }

    function test_aShortScanIsNotAnnounced() {
        fake.scanning = true
        verify(!row.shown)
        compare(row.height, 0)
        wait(100)
        fake.scanning = false
        wait(500)
        verify(!row.shown)
    }

    function test_aLongScanIs() {
        fake.scanning = true
        tryCompare(row, "shown", true, 2000)
        verify(row.height > 0)
        fake.scanning = false
        verify(!row.shown)
        // the next one waits again
        fake.scanning = true
        verify(!row.shown)
    }

    function test_actionsAreShownAtOnce() {
        fake.busy = true
        verify(row.shown)
    }
}
