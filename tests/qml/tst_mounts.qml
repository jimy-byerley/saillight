import QtQuick 2.0
import QtTest 1.0
import "../../qml/components"

// the storages are listed while nothing is selected, and a tap opens one
TestCase {
    name: "mounts"

    QtObject {
        id: fake
        property int selected: -1
        property var mounts: []
        property var calls: []
        function setFolder(path) { calls.push(path) }
    }

    Component {
        id: component
        Mounts { width: 600 }
    }

    property var mounts: null

    function init() {
        fake.calls = []
        fake.selected = -1
        fake.mounts = [
            { path: "/", total: 1000, used: 600, available: 300 },
            { path: "/home", total: 5000, used: 0, available: 5000 }
        ]
        mounts = component.createObject(null, { model: fake })
    }

    function cleanup() {
        mounts.destroy()
    }

    function entries(parent, found) {
        found = found || []
        for (var index = 0; index < parent.children.length; index++) {
            var child = parent.children[index]
            if (child.fill !== undefined && child.path !== undefined)
                found.push(child)
            entries(child, found)
        }
        return found
    }

    function test_oneGaugePerStorage() {
        verify(mounts.shown)
        var found = entries(mounts)
        compare(found.length, 2)
        compare(found[0].path, "/")
        fuzzyCompare(found[0].fill, 0.6, 1e-9)
        compare(found[1].fill, 0)
    }

    function test_hiddenBySelection() {
        fake.selected = 3
        verify(!mounts.shown)
        verify(!mounts.visible)
        compare(entries(mounts).length, 0)
    }

    function test_tapOpens() {
        entries(mounts)[1].clicked(null)
        compare(fake.calls.length, 1)
        compare(fake.calls[0], "/home")
    }

    function test_nothingKnown() {
        fake.mounts = []
        verify(!mounts.shown)
        fake.mounts = [{ path: "/", total: 0, used: 0, available: 0 }]
        compare(entries(mounts)[0].fill, 0)
    }
}
