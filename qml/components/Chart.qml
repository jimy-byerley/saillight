import QtQuick 2.0
import Sailfish.Silica 1.0
import "../js/logic.js" as Logic

// draws the sectors computed by the model, and reports press, slide and release.
// no geometry is computed here
Item {
    id: chart

    // the ChartModel, or anything with its properties and methods
    property var model
    readonly property real radius: Math.min(width, height) / 2
    readonly property real centreX: width / 2
    readonly property real centreY: height / 2

    // what the canvases draw: they repaint when these change
    property var sectors: model ? model.sectors : []
    property int hovered: model ? model.hovered : -1
    property int selected: model ? model.selected : -1
    // time taken by the last painting of all the sectors, milliseconds
    property int paintTime: 0

    readonly property color unreadableColor: "#ff5a4d"
    readonly property color mountColor: Theme.secondaryHighlightColor
    // between the sectors, the color of what is behind the chart
    readonly property color outlineColor: Theme.colorScheme === Theme.LightOnDark ? "#000000" : "#ffffff"

    onSectorsChanged: { sectorCanvas.requestPaint(); overlay.requestPaint() }
    onHoveredChanged: overlay.requestPaint()
    onSelectedChanged: overlay.requestPaint()
    onRadiusChanged: { sectorCanvas.requestPaint(); overlay.requestPaint() }

    // outline of a sector as the current path of a canvas context
    function trace(context, sector) {
        var start = Logic.canvasAngle(sector.start)
        var end = Logic.canvasAngle(sector.end)
        context.beginPath()
        if (sector.kind !== 0) {
            // a marker has no width: a line across its ring
            context.moveTo(centreX + Math.cos(start) * sector.inner * radius,
                           centreY + Math.sin(start) * sector.inner * radius)
            context.lineTo(centreX + Math.cos(start) * sector.outer * radius,
                           centreY + Math.sin(start) * sector.outer * radius)
            return
        }
        context.arc(centreX, centreY, sector.outer * radius, start, end, false)
        context.arc(centreX, centreY, sector.inner * radius, end, start, true)
        context.closePath()
    }

    function markerColor(sector) {
        return Logic.cssColor(sector.kind === 1 ? unreadableColor : mountColor)
    }

    Canvas {
        id: sectorCanvas
        anchors.fill: parent

        onPaint: {
            var started = Date.now()
            var context = getContext("2d")
            context.reset()
            var sectors = chart.sectors
            // thin lines of the background between the sectors
            context.lineWidth = Math.max(1, Math.round(Theme.pixelRatio * 1.5))
            context.strokeStyle = Logic.cssColor(chart.outlineColor)
            var index, sector
            for (index = 0; index < sectors.length; index++) {
                sector = sectors[index]
                if (sector.kind !== 0)
                    continue
                var color = Logic.sectorHsl(sector.group, sector.ring, sector.folder)
                context.fillStyle = Logic.cssColor(Qt.hsla(color[0], color[1], color[2], 1))
                chart.trace(context, sector)
                context.fill()
                context.stroke()
            }
            // markers above everything
            context.lineCap = "round"
            context.lineWidth = Theme.paddingSmall
            for (index = 0; index < sectors.length; index++) {
                sector = sectors[index]
                if (sector.kind === 0)
                    continue
                context.strokeStyle = chart.markerColor(sector)
                chart.trace(context, sector)
                context.stroke()
            }
            chart.paintTime = Date.now() - started
        }
    }

    // the hovered and selected sectors are drawn apart, so sliding does not repaint the whole chart
    Canvas {
        id: overlay
        anchors.fill: parent

        function highlight(context, sector, fill, line) {
            chart.trace(context, sector)
            if (sector.kind !== 0) {
                context.lineCap = "round"
                context.lineWidth = Theme.paddingMedium
                context.strokeStyle = line
                context.stroke()
                return
            }
            context.fillStyle = fill
            context.fill()
            context.lineWidth = Math.max(2, Math.round(Theme.pixelRatio * 4))
            context.strokeStyle = line
            context.stroke()
        }

        onPaint: {
            var context = getContext("2d")
            context.reset()
            var sectors = chart.sectors
            if (chart.selected >= 0 && chart.selected < sectors.length)
                highlight(context, sectors[chart.selected],
                          Logic.cssColor(Theme.highlightColor, 0.35), Logic.cssColor(Theme.highlightColor))
            if (chart.hovered >= 0 && chart.hovered < sectors.length && chart.hovered !== chart.selected)
                highlight(context, sectors[chart.hovered],
                          Logic.cssColor(Theme.primaryColor, 0.35), Logic.cssColor(Theme.primaryColor))
        }
    }

    // a finger at a position of this item. press tells whether it is on the chart
    function press(x, y) {
        var relativeX = (x - centreX) / radius, relativeY = (y - centreY) / radius
        if (!model || radius <= 0 || relativeX * relativeX + relativeY * relativeY > 1)
            return false
        model.hoverAt(relativeX, relativeY)
        return true
    }
    function slide(x, y) {
        if (model && radius > 0)
            model.hoverAt((x - centreX) / radius, (y - centreY) / radius)
    }
    function release(x, y) {
        if (model && radius > 0)
            model.releaseAt((x - centreX) / radius, (y - centreY) / radius)
    }
    function cancel() {
        if (model)
            model.hoverNothing()
    }

    MouseArea {
        anchors.fill: parent
        // a slide on the chart changes what is going to be selected, it does not scroll the page
        preventStealing: true

        // the corners belong to the page
        onPressed: mouse.accepted = chart.press(mouse.x, mouse.y)
        onPositionChanged: if (pressed) chart.slide(mouse.x, mouse.y)
        onReleased: chart.release(mouse.x, mouse.y)
        onCanceled: chart.cancel()
    }
}
