import QtQuick 2.0
import Sailfish.Silica 1.0
import "../js/logic.js" as Logic

// names of the items of the current folder around the chart, with arrows to their sectors,
// at the positions computed by the model
Item {
    id: root

    // the ChartModel, or anything with its properties and methods
    property var model
    // centre of the chart in this item: the positions given by the model are relative to it
    property real centreX: width / 2
    property real centreY: height / 2
    // height of a line of text, the one given to the model
    property real line: Theme.fontSizeExtraSmall * 1.5

    property var labels: model ? model.labels : []
    property int selected: model ? model.selected : -1
    property int hovered: model ? model.hovered : -1

    onLabelsChanged: arrows.requestPaint()
    onCentreXChanged: arrows.requestPaint()
    onCentreYChanged: arrows.requestPaint()
    onSelectedChanged: arrows.requestPaint()
    onHoveredChanged: arrows.requestPaint()

    // a canvas loses what it drew while the app is in the background
    Connections {
        target: Qt.application
        onActiveChanged: if (Qt.application.active) arrows.requestPaint()
    }

    function colorOf(sector) {
        if (sector === selected)
            return Theme.highlightColor
        return sector === hovered ? Theme.primaryColor : Theme.secondaryColor
    }

    Canvas {
        id: arrows
        anchors.fill: parent
        onAvailableChanged: if (available) requestPaint()

        onPaint: {
            var context = getContext("2d")
            context.reset()
            var labels = root.labels
            context.lineWidth = Math.max(1, Math.round(Theme.pixelRatio * 2))
            context.lineJoin = "round"
            for (var index = 0; index < labels.length; index++) {
                var label = labels[index]
                var color = Logic.cssColor(root.colorOf(label.sector))
                context.strokeStyle = color
                context.fillStyle = color
                context.beginPath()
                context.moveTo(root.centreX + label.ax, root.centreY + label.ay)
                context.lineTo(root.centreX + label.ex, root.centreY + label.ey)
                context.lineTo(root.centreX + label.tx, root.centreY + label.ty)
                context.stroke()
                // the arrow ends with a dot in its sector
                context.beginPath()
                context.arc(root.centreX + label.tx, root.centreY + label.ty,
                            Theme.paddingSmall * 0.8, 0, 2 * Math.PI, false)
                context.fill()
            }
        }
    }

    Repeater {
        model: root.labels

        MouseArea {
            // a tap on a name selects its sector
            x: root.centreX + (modelData.right ? modelData.x - modelData.width : modelData.x)
            y: root.centreY + modelData.y - root.line / 2
            width: modelData.width
            height: root.line
            onClicked: if (root.model) root.model.select(modelData.sector)

            Label {
                anchors.fill: parent
                text: modelData.text
                font.pixelSize: Theme.fontSizeExtraSmall
                color: root.colorOf(modelData.sector)
                // a name too long for its room keeps its start
                horizontalAlignment: modelData.right && implicitWidth <= width
                                     ? Text.AlignRight : Text.AlignLeft
                verticalAlignment: Text.AlignVCenter
                truncationMode: TruncationMode.Fade
            }
        }
    }
}
