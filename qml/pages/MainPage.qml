import QtQuick 2.0
import Sailfish.Silica 1.0
import Saillight 1.0
import "../components"

// the only page: path, chart with its labels, details of the selected sector
Page {
    id: page
    allowedOrientations: Orientation.All

    ChartModel { id: chartModel }

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: content.height

        PullDownMenu {
            MenuItem { text: "Refresh" }
        }

        Column {
            id: content
            width: parent.width

            PathBar {
                width: parent.width
                path: chartModel.folder
            }
            Item {
                width: parent.width
                height: width

                ChartLabels { anchors.fill: parent }
                Chart {
                    anchors.fill: parent
                    anchors.margins: Theme.horizontalPageMargin
                }
            }
            ProgressRow { width: parent.width }
            Details { width: parent.width }
        }
    }
}
