import QtQuick 2.0
import QtTest 1.0
import "../../qml/components"

// the path bar shows one level per folder and tells which one is clicked.
// the build target has no screen: clicks are made by emitting the signal of what is clicked
TestCase {
    name: "pathbar"
    Component {
        id: component
        PathBar { width: 600; path: "/home/user/Pictures" }
    }

    property var bar: null

    SignalSpy {
        id: levelSpy
        signalName: "levelClicked"
    }

    SignalSpy {
        id: staleSpy
        signalName: "staleClicked"
    }

    function init() {
        bar = component.createObject(null)
        levelSpy.target = bar
        staleSpy.target = bar
        levelSpy.clear()
        staleSpy.clear()
    }

    function cleanup() {
        levelSpy.target = null
        staleSpy.target = null
        bar.destroy()
    }

    // everything in the bar that can be clicked and is shown, in order
    function clickables(parent, found) {
        found = found || []
        for (var index = 0; index < parent.children.length; index++) {
            var child = parent.children[index]
            if (child.clicked !== undefined && child.visible)
                found.push(child)
            clickables(child, found)
        }
        return found
    }

    // labels of the levels, in order
    function labels(parent, found) {
        found = found || []
        for (var index = 0; index < parent.children.length; index++) {
            var child = parent.children[index]
            if (typeof child.text === "string" && child.text !== "" && child.truncationMode !== undefined)
                found.push(child)
            labels(child, found)
        }
        return found
    }

    function texts() {
        return labels(bar).map(function(label) { return label.text })
    }

    function test_oneLabelPerLevel() {
        compare(texts(), ["/", "home/", "user/", "Pictures/"])
        bar.path = "/"
        compare(texts(), ["/"])
        bar.path = "/srv"
        compare(texts(), ["/", "srv/"])
    }

    function test_clickGivesThePathOfTheLevel() {
        var found = clickables(bar)
        found[1].clicked(null)
        compare(levelSpy.count, 1)
        compare(levelSpy.signalArguments[0][0], "/home")
        found[0].clicked(null)
        compare(levelSpy.signalArguments[1][0], "/")
        found[3].clicked(null)
        compare(levelSpy.signalArguments[2][0], "/home/user/Pictures")
    }

    function test_warningOnlyWhenStale() {
        // the levels are all there is to click while the folder is up to date
        compare(clickables(bar).length, 4)
        bar.stale = true
        var found = clickables(bar)
        compare(found.length, 5)
        found[4].clicked(null)
        compare(staleSpy.count, 1)
        compare(levelSpy.count, 0)
    }
}
