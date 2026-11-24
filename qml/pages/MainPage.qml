import QtQuick 2.0
import Sailfish.Silica 1.0
import Saillight 1.0
import "../components"
import "../js/logic.js" as Logic

// the only page: path, chart with its labels, details of the selected sector
Page {
    id: page
    allowedOrientations: Orientation.All

    // height of a line of names around the chart
    readonly property real labelLine: Math.round(Theme.fontSizeExtraSmall * 1.5)
    // portrait: most lines of names above the chart, and as many below
    readonly property int labelRows: 5

    // in landscape the chart and its names take the left of the page, the details the right
    readonly property real blockWidth: isLandscape ? Math.round(width * 0.66) : width
    readonly property real chartRadius: isLandscape
        ? Math.max(0, (height - pathBar.height - progress.height) / 2 - Theme.paddingMedium)
        : Math.max(0, width / 2 - Theme.horizontalPageMargin)
    // landscape: width of the columns of names on each side of the chart
    readonly property real sideWidth: Math.max(0, blockWidth / 2 - chartRadius - labelLine - Theme.paddingMedium)

    function updateLabelArea() {
        if (isLandscape)
            chartModel.setLabelArea(true, chartRadius, labelLine, sideWidth,
                                    chartRadius + Theme.paddingMedium, 0)
        else
            chartModel.setLabelArea(false, chartRadius, labelLine, width / 2 - Theme.paddingMedium,
                                    0, labelRows)
    }

    // the area is given once the sizes are settled: the answer of the model changes the
    // height of the chart block, which must not happen while that height is being computed
    Timer {
        id: areaTimer
        interval: 0
        onTriggered: page.updateLabelArea()
    }
    onChartRadiusChanged: areaTimer.restart()
    onSideWidthChanged: areaTimer.restart()
    onIsLandscapeChanged: areaTimer.restart()
    Component.onCompleted: {
        updateLabelArea()
        chartModel.start()
    }

    // the sizes shown are being computed again
    readonly property bool rescanning: chartModel.known && (chartModel.scanning || rescanShown.running)
    Timer {
        id: rescanShown
        interval: 800
    }
    readonly property bool scanning: chartModel.scanning
    onScanningChanged: if (scanning && chartModel.known) rescanShown.restart()

    ChartModel {
        id: chartModel
        onFailed: Notices.show(message, Notice.Long, Notice.Bottom)
    }

    // the cover shows the same folder
    property alias model: chartModel

    // back in front: what the chart shows may be old
    Connections {
        target: Qt.application
        onActiveChanged: if (Qt.application.active) chartModel.checkStale()
    }

    // hands a file or folder to the application the system has for it
    function openExternally(path) {
        if (!Qt.openUrlExternally(Logic.fileUrl(path)))
            Notices.show("No application to open " + path, Notice.Long, Notice.Bottom)
    }

    RemorsePopup {
        id: remorse
    }

    SilicaFlickable {
        id: flickable
        anchors.fill: parent
        contentHeight: Math.max(block.y + block.height,
                                details.y + (details.visible ? details.height : mounts.height))

        PullDownMenu {
            MenuItem {
                visible: chartModel.depth > 1
                text: "Fewer levels (" + (chartModel.depth - 1) + ")"
                onClicked: chartModel.setDepth(chartModel.depth - 1)
            }
            MenuItem {
                visible: chartModel.depth < chartModel.maxDepth
                text: "More levels (" + (chartModel.depth + 1) + ")"
                onClicked: chartModel.setDepth(chartModel.depth + 1)
            }
            MenuItem {
                text: "Open in file manager"
                onClicked: page.openExternally(chartModel.folder)
            }
            MenuItem {
                text: "Refresh"
                onClicked: chartModel.refresh()
            }
        }

        PathBar {
            id: pathBar
            width: page.width
            path: chartModel.folder
            stale: chartModel.stale
            onLevelClicked: chartModel.setFolder(path)
            onStaleClicked: chartModel.refresh()
        }

        ProgressRow {
            id: progress
            y: pathBar.height
            width: page.width
            model: chartModel
        }

        // the chart with its names around
        Item {
            id: block
            y: progress.y + progress.height
            width: page.blockWidth
            height: page.isLandscape
                ? 2 * (page.chartRadius + Theme.paddingMedium)
                : chartModel.labelsAbove + 2 * page.chartRadius + chartModel.labelsBelow + 2 * Theme.paddingMedium

            readonly property real centreX: width / 2
            readonly property real centreY: page.isLandscape
                ? height / 2
                : Theme.paddingMedium + chartModel.labelsAbove + page.chartRadius

            // a tap around the chart clears the selection
            MouseArea {
                anchors.fill: parent
                onClicked: chartModel.select(-1)
            }

            Chart {
                id: chart
                model: chartModel
                x: block.centreX - page.chartRadius
                y: block.centreY - page.chartRadius
                width: 2 * page.chartRadius
                height: 2 * page.chartRadius
            }

            // above the chart, so the arrows are drawn over the sectors
            ChartLabels {
                anchors.fill: parent
                model: chartModel
                centreX: block.centreX
                centreY: block.centreY
                line: page.labelLine
            }

            // the centre of the chart: the current folder, a tap goes to its parent
            Column {
                id: centre
                width: page.chartRadius * 0.4
                x: block.centreX - width / 2
                y: block.centreY - height / 2

                Image {
                    anchors.horizontalCenter: parent.horizontalCenter
                    visible: chartModel.hasParent
                    source: "image://theme/icon-s-arrow?" + Theme.secondaryColor
                    rotation: 180
                }
                Label {
                    width: parent.width
                    horizontalAlignment: Text.AlignHCenter
                    text: chartModel.folderName
                    font.pixelSize: Theme.fontSizeExtraSmall
                    truncationMode: TruncationMode.Fade
                }
                // sizes being computed again: shown long enough to be seen, as a scan can
                // take less than a blink
                BusyIndicator {
                    anchors.horizontalCenter: parent.horizontalCenter
                    size: BusyIndicatorSize.Small
                    visible: page.rescanning
                    running: visible
                }
                Label {
                    width: parent.width
                    horizontalAlignment: Text.AlignHCenter
                    visible: chartModel.known && !page.rescanning
                    text: Logic.formatSize(chartModel.totalSize)
                    color: Theme.highlightColor
                    font.pixelSize: Theme.fontSizeSmall
                }
            }

            BusyIndicator {
                anchors.centerIn: chart
                size: BusyIndicatorSize.Large
                running: chartModel.scanning && !chartModel.known
            }
        }

        Details {
            id: details
            model: chartModel
            x: page.isLandscape ? page.blockWidth : 0
            y: page.isLandscape ? progress.y + progress.height + Theme.paddingMedium
                                : block.y + block.height
            width: page.width - x
            onOpenRequested: page.openExternally(path)
            onDeleteRequested: remorse.execute("Deleting " + name, function() {
                chartModel.deleteItem(path)
            })
        }

        // without a selection, the storages take the place of the details
        Mounts {
            id: mounts
            model: chartModel
            x: details.x
            y: details.y
            width: details.width
        }

        VerticalScrollDecorator { }
    }
}
