import QtQuick 2.0
import Sailfish.Silica 1.0
import "../js/logic.js" as Logic

// a storage as a ring of three sectors, clockwise from the top: what a folder takes in it,
// the rest of what is used, what is free
Item {
    id: root

    // bytes of the folder, bytes used in the storage, bytes the storage can hold
    property real folder: 0
    property real used: 0
    property real total: 0
    property real thickness: Theme.paddingLarge * 0.8
    readonly property var shares: Logic.volumeShares(folder, used, total)
    // what is drawn for the folder: a folder too small to be seen still gets a mark
    readonly property real folderShown: shares[0] > 0 ? Math.max(shares[0], 0.012) : 0

    readonly property color folderColor: Theme.highlightColor
    readonly property color otherColor: Theme.rgba(Theme.primaryColor, 0.55)
    readonly property color freeColor: Theme.rgba(Theme.primaryColor, 0.2)

    onSharesChanged: canvas.requestPaint()
    onThicknessChanged: canvas.requestPaint()
    onFolderColorChanged: canvas.requestPaint()

    function repaint() {
        canvas.requestPaint()
    }

    Canvas {
        id: canvas
        anchors.fill: parent
        onAvailableChanged: if (available) requestPaint()

        onPaint: {
            var context = getContext("2d")
            context.reset()
            var radius = Math.min(width, height) / 2 - root.thickness / 2
            if (radius <= 0)
                return
            context.lineWidth = root.thickness
            var own = root.folderShown
            var taken = Math.max(own, root.shares[0] + root.shares[1])
            var parts = [[0, own, root.folderColor], [own, taken, root.otherColor], [taken, 1, root.freeColor]]
            for (var index = 0; index < parts.length; index++) {
                if (parts[index][1] <= parts[index][0])
                    continue
                context.strokeStyle = Logic.cssColor(parts[index][2])
                context.beginPath()
                context.arc(width / 2, height / 2, radius, Logic.canvasAngle(parts[index][0]),
                            Logic.canvasAngle(parts[index][1]), false)
                context.stroke()
            }
        }
    }
}
