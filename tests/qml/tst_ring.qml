import QtQuick 2.0
import QtTest 1.0
import "../../qml/components"

// the ring of the cover: a folder in its storage
TestCase {
    name: "ring"

    Component {
        id: component
        UsageRing { width: 200; height: 200 }
    }

    function test_shares() {
        var ring = component.createObject(null, { folder: 100, used: 400, total: 1000 })
        fuzzyCompare(ring.shares[0], 0.1, 1e-9)
        fuzzyCompare(ring.folderShown, 0.1, 1e-9)
        ring.destroy()
    }

    function test_aSmallFolderIsStillMarked() {
        var ring = component.createObject(null, { folder: 1, used: 400000, total: 1000000 })
        verify(ring.folderShown >= 0.01)
        ring.folder = 0
        compare(ring.folderShown, 0)
        ring.destroy()
    }

    function test_unknownStorage() {
        var ring = component.createObject(null, { folder: 100 })
        compare(ring.shares, [0, 0, 1])
        ring.destroy()
    }
}
