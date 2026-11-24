import QtQuick 2.0
import QtTest 1.0
import "../../qml/components"

// the details panel follows the selection, and asks before anything is removed.
// the build target has no screen: buttons are clicked by emitting their signal, when enabled
TestCase {
    name: "details"
    QtObject {
        id: fake
        property int selected: -1
        property bool busy: false
        property bool scanning: false
        property var details: ({})
        property var calls: []
        function enterSelected() { calls.push(["enter"]) }
        function compressItem(path, format) { calls.push(["compress", path, format]) }
        function setFolder(path) { calls.push(["folder", path]) }
    }

    Component {
        id: component
        Details { width: 600 }
    }

    property var details: null

    SignalSpy {
        id: deleteSpy
        signalName: "deleteRequested"
    }

    SignalSpy {
        id: openSpy
        signalName: "openRequested"
    }

    function item(kind, refusal) {
        return {
            kind: kind, name: "album", path: "/home/user/album", size: 1500000, date: 0,
            permissions: "rwxr-xr-x", owner: "user", count: 3, enterable: kind === "folder",
            blocked: [], mounts: [], refusal: refusal || ""
        }
    }

    function show(details) {
        fake.details = details
        fake.selected = 0
    }

    function init() {
        fake.calls = []
        fake.busy = false
        fake.scanning = false
        fake.selected = -1
        fake.details = ({})
        details = component.createObject(null, { model: fake })
        deleteSpy.target = details
        deleteSpy.clear()
        openSpy.target = details
        openSpy.clear()
    }

    function cleanup() {
        deleteSpy.target = null
        openSpy.target = null
        details.destroy()
    }

    // what a tap does: nothing on a disabled button
    function click(button) {
        verify(button !== null)
        if (button.enabled)
            button.clicked(null)
    }

    // the visible buttons of a given action, among everything in the panel
    function buttons(parent, text, found) {
        found = found || []
        for (var index = 0; index < parent.children.length; index++) {
            var child = parent.children[index]
            if (child.action === text && child.visible)
                found.push(child)
            buttons(child, text, found)
        }
        return found
    }

    function button(text) {
        var found = buttons(details, text)
        return found.length > 0 ? found[0] : null
    }

    function test_hiddenWithoutSelection() {
        verify(!details.shown)
        verify(!details.visible)
        verify(!details.allowed)
    }

    function test_folder() {
        show(item("folder"))
        verify(details.visible)
        verify(!details.marker)
        verify(details.allowed)
        verify(button("open") !== null)
        verify(button("delete").enabled)
        // folders go through tar, except for zip
        compare(button("zip").caption, "zip")
        compare(button("gz").caption, "tar.gz")
        compare(button("7z").caption, "tar.7z")
    }

    function test_file() {
        show(item("file"))
        verify(button("open") !== null)
        compare(button("gz").caption, "gz")
        compare(button("7z").caption, "7z")
    }

    function test_linksAreNotCompressed() {
        show(item("link"))
        verify(button("delete").enabled)
        verify(button("zip") === null)
        verify(button("open") === null)
    }

    function test_deleteAsksThePage() {
        show(item("file"))
        click(button("delete"))
        compare(deleteSpy.count, 1)
        compare(deleteSpy.signalArguments[0][0], "/home/user/album")
        compare(deleteSpy.signalArguments[0][1], "album")
        // nothing was asked to the model
        compare(fake.calls.length, 0)
    }

    function test_compressNamesThePath() {
        show(item("folder"))
        click(button("7z"))
        compare(fake.calls.length, 1)
        compare(fake.calls[0][0], "compress")
        compare(fake.calls[0][1], "/home/user/album")
        compare(fake.calls[0][2], "7z")
    }

    function test_open() {
        show(item("folder"))
        click(button("open"))
        compare(fake.calls[0][0], "enter")
        compare(openSpy.count, 0)
    }

    function test_openFileAsksThePage() {
        show(item("file"))
        // a file can be opened even while the app works
        fake.busy = true
        verify(button("open").enabled)
        click(button("open"))
        compare(openSpy.count, 1)
        compare(openSpy.signalArguments[0][0], "/home/user/album")
        compare(fake.calls.length, 0)
    }

    function test_refusedItemsCannotBeRemoved() {
        show(item("folder", "The home folder cannot be removed"))
        verify(!details.allowed)
        verify(!button("delete").enabled)
        verify(!button("zip").enabled)
        // but can still be opened
        verify(button("open").enabled)
        click(button("delete"))
        click(button("zip"))
        compare(deleteSpy.count, 0)
        compare(fake.calls.length, 0)
    }

    function test_nothingIsRemovedWhileWorking() {
        show(item("file"))
        fake.busy = true
        verify(!button("delete").enabled)
        verify(!button("zip").enabled)
        fake.busy = false
        fake.scanning = true
        verify(!button("delete").enabled)
        fake.scanning = false
        verify(button("delete").enabled)
    }

    function test_marker() {
        var marker = item("unreadable")
        marker.blocked = ["/home/user/album/locked"]
        marker.mounts = ["/home/user/album/card"]
        show(marker)
        verify(details.marker)
        verify(!details.allowed)
        verify(button("delete") === null)
        verify(button("zip") === null)
        verify(button("open") === null)
    }
}
