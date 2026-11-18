import QtQuick 2.0
import QtTest 1.0
import "../../qml/components"

// the chart reports touches to its model in chart units, and leaves its corners to the page.
// the build target has no screen, so touches are given to the functions the touch area calls
TestCase {
    name: "chart"

    // stands for the ChartModel, and records what it is asked
    QtObject {
        id: fake
        property var sectors: [
            { start: 0, end: 0.5, inner: 0.24, outer: 0.62, ring: 1, group: 0, kind: 0, folder: true },
            { start: 0.5, end: 0.9, inner: 0.24, outer: 0.62, ring: 1, group: 1, kind: 0, folder: false },
            { start: 1, end: 1, inner: 0.24, outer: 0.62, ring: 1, group: 0, kind: 1, folder: false }
        ]
        property int hovered: -1
        property int selected: -1
        property var calls: []
        function hoverAt(x, y) { calls.push(["hover", x, y]) }
        function releaseAt(x, y) { calls.push(["release", x, y]); return true }
        function hoverNothing() { calls.push(["nothing"]) }
    }

    Component {
        id: component
        Chart { width: 400; height: 400 }
    }

    property var chart: null

    function init() {
        fake.calls = []
        chart = component.createObject(null, { model: fake })
    }

    function cleanup() {
        chart.destroy()
    }

    function last() {
        return fake.calls[fake.calls.length - 1]
    }

    function test_geometry() {
        compare(chart.radius, 200)
        compare(chart.centreX, 200)
        compare(chart.centreY, 200)
        chart.width = 300
        compare(chart.radius, 150)
        compare(chart.centreY, 200)
    }

    function test_modelIsFollowed() {
        compare(chart.sectors.length, 3)
        compare(chart.hovered, -1)
        fake.hovered = 1
        compare(chart.hovered, 1)
        fake.selected = 0
        compare(chart.selected, 0)
        fake.hovered = -1
        fake.selected = -1
    }

    function test_pressSlideRelease() {
        // right of the centre, half way to the rim
        verify(chart.press(300, 200))
        compare(fake.calls.length, 1)
        compare(last()[0], "hover")
        fuzzyCompare(last()[1], 0.5, 1e-6)
        fuzzyCompare(last()[2], 0, 1e-6)

        // sliding to under the centre
        chart.slide(200, 350)
        compare(last()[0], "hover")
        fuzzyCompare(last()[1], 0, 1e-6)
        fuzzyCompare(last()[2], 0.75, 1e-6)

        chart.release(200, 350)
        compare(last()[0], "release")
        fuzzyCompare(last()[1], 0, 1e-6)
        fuzzyCompare(last()[2], 0.75, 1e-6)
        compare(fake.calls.length, 3)
    }

    function test_slidingOutIsReportedAsOutside() {
        verify(chart.press(200, 100))
        chart.slide(395, 5)
        chart.release(395, 5)
        compare(last()[0], "release")
        // out of the unit disc: the model takes it as a cancellation
        verify(last()[1] * last()[1] + last()[2] * last()[2] > 1)
    }

    function test_cornersAreNotTheChart() {
        verify(!chart.press(10, 10))
        verify(!chart.press(390, 390))
        compare(fake.calls.length, 0)
        // the rim is
        verify(chart.press(200, 1))
    }

    function test_cancel() {
        chart.press(300, 200)
        chart.cancel()
        compare(last()[0], "nothing")
    }

    function test_withoutModel() {
        chart.model = null
        compare(chart.hovered, -1)
        compare(chart.sectors.length, 0)
        verify(!chart.press(300, 200))
        chart.slide(300, 200)
        chart.release(300, 200)
        chart.cancel()
        compare(fake.calls.length, 0)
    }

    function test_outlineOfASector() {
        // a context recording what is drawn
        var drawn = []
        var context = {
            beginPath: function() { drawn.push("begin") },
            closePath: function() { drawn.push("close") },
            moveTo: function(x, y) { drawn.push(["move", x, y]) },
            lineTo: function(x, y) { drawn.push(["line", x, y]) },
            arc: function(x, y, radius, start, end, reverse) { drawn.push(["arc", radius, start, end, reverse]) }
        }
        chart.trace(context, fake.sectors[0])
        compare(drawn.length, 4)
        compare(drawn[1][0], "arc")
        // outer arc forward from the top to the bottom, inner arc back
        fuzzyCompare(drawn[1][1], 0.62 * 200, 1e-6)
        fuzzyCompare(drawn[1][2], -Math.PI / 2, 1e-6)
        fuzzyCompare(drawn[1][3], Math.PI / 2, 1e-6)
        compare(drawn[1][4], false)
        fuzzyCompare(drawn[2][1], 0.24 * 200, 1e-6)
        compare(drawn[2][4], true)
        compare(drawn[3], "close")

        // a marker is a line across its ring, at the top here
        drawn = []
        chart.trace(context, fake.sectors[2])
        compare(drawn.length, 3)
        compare(drawn[1][0], "move")
        fuzzyCompare(drawn[1][1], 200, 1e-6)
        fuzzyCompare(drawn[1][2], 200 - 0.24 * 200, 1e-6)
        compare(drawn[2][0], "line")
        fuzzyCompare(drawn[2][2], 200 - 0.62 * 200, 1e-6)
    }
}
