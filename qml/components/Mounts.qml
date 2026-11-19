import QtQuick 2.6
import Sailfish.Silica 1.0
import "../js/logic.js" as Logic

// how full each storage of the device is, shown while nothing is selected.
// a tap on one makes it the current folder
Column {
    id: root

    // the ChartModel, or anything with its properties and methods
    property var model
    property var mounts: model ? model.mounts : []
    readonly property bool shown: model ? model.selected < 0 && mounts.length > 0 : false

    visible: shown

    Repeater {
        model: root.shown ? root.mounts : []

        BackgroundItem {
            id: entry
            // for whoever looks for it
            property string path: modelData.path
            readonly property real fill: Logic.fraction(modelData.used, modelData.total)

            width: root.width
            height: content.height + 2 * Theme.paddingMedium
            onClicked: root.model.setFolder(modelData.path)

            Column {
                id: content
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * x
                anchors.verticalCenter: parent.verticalCenter
                spacing: Theme.paddingSmall

                Item {
                    width: parent.width
                    height: Math.max(name.height, figures.height)

                    Label {
                        id: name
                        width: parent.width - figures.width - Theme.paddingMedium
                        text: modelData.path
                        color: entry.highlighted ? Theme.highlightColor : Theme.primaryColor
                        truncationMode: TruncationMode.Fade
                    }
                    Label {
                        id: figures
                        anchors { right: parent.right; baseline: name.baseline }
                        text: Logic.usageLabel(modelData.used, modelData.total)
                        color: Theme.secondaryColor
                        font.pixelSize: Theme.fontSizeExtraSmall
                    }
                }

                // the gauge
                Rectangle {
                    width: parent.width
                    height: Theme.paddingSmall
                    radius: height / 2
                    color: Theme.rgba(Theme.primaryColor, 0.2)

                    Rectangle {
                        width: Math.max(parent.height, parent.width * entry.fill)
                        height: parent.height
                        radius: parent.radius
                        visible: entry.fill > 0
                        color: Theme.highlightColor
                    }
                }
            }
        }
    }
}
